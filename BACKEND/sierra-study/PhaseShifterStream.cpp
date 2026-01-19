// PhaseShifterStream.cpp
// Sierra Chart ACSIL Study - Streams tick data to PhaseShifter Rust server
//
// Installation:
// 1. Copy this file to: Sierra Chart\ACS_Source\
// 2. Build via Analysis > Build Custom Studies DLL
// 3. Add study to chart: Analysis > Studies > Add Custom Study > PhaseShifterStream
//
// The study connects to localhost:9000 and streams tick data in binary format.

// IMPORTANT: Define WIN32_LEAN_AND_MEAN before any includes to prevent winsock.h
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif

// Prevent winsock.h from being included
#ifndef _WINSOCKAPI_
#define _WINSOCKAPI_
#endif

#include <winsock2.h>
#include <ws2tcpip.h>

// Now include Sierra Chart header
#include "sierrachart.h"

#pragma comment(lib, "ws2_32.lib")

SCDLLName("PhaseShifter Stream")

// Binary message format for tick data (32 bytes)
#pragma pack(push, 1)
struct TickMessage {
    uint8_t  msg_type;      // 1 = tick, 2 = bar close, 255 = heartbeat
    uint8_t  reserved[3];   // Padding for alignment
    uint32_t symbol_id;     // Symbol identifier (hash of symbol string)
    double   price;         // Trade price
    double   volume;        // Trade volume
    int64_t  timestamp_us;  // Microseconds since Unix epoch
};
#pragma pack(pop)

// Global socket state (shared across all instances)
static SOCKET g_Socket = INVALID_SOCKET;
static bool g_WsaInitialized = false;
static CRITICAL_SECTION g_SocketLock;
static bool g_LockInitialized = false;

// Simple hash function for symbol string (matches Rust implementation)
uint32_t HashSymbol(const char* symbol) {
    uint32_t hash = 5381;
    int c;
    while ((c = *symbol++)) {
        hash = ((hash << 5) + hash) + c;
    }
    return hash;
}

// Initialize Winsock (called once)
bool InitWinsock() {
    if (g_WsaInitialized) return true;

    WSADATA wsaData;
    if (WSAStartup(MAKEWORD(2, 2), &wsaData) != 0) {
        return false;
    }
    g_WsaInitialized = true;
    return true;
}

// Connect to the server
bool ConnectToServer(int port, SCStudyInterfaceRef sc) {
    if (g_Socket != INVALID_SOCKET) {
        return true;  // Already connected
    }

    if (!InitWinsock()) {
        sc.AddMessageToLog("PhaseShifter: WSAStartup failed", 1);
        return false;
    }

    // Create socket
    g_Socket = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (g_Socket == INVALID_SOCKET) {
        sc.AddMessageToLog("PhaseShifter: Socket creation failed", 1);
        return false;
    }

    // Set non-blocking for connect with timeout
    u_long mode = 1;
    ioctlsocket(g_Socket, FIONBIO, &mode);

    // Connect to localhost
    struct sockaddr_in serverAddr;
    memset(&serverAddr, 0, sizeof(serverAddr));
    serverAddr.sin_family = AF_INET;
    serverAddr.sin_port = htons((u_short)port);
    serverAddr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);  // 127.0.0.1

    int result = connect(g_Socket, (struct sockaddr*)&serverAddr, sizeof(serverAddr));

    if (result == SOCKET_ERROR) {
        int error = WSAGetLastError();
        if (error == WSAEWOULDBLOCK) {
            // Wait for connection with timeout
            fd_set writeSet;
            FD_ZERO(&writeSet);
            FD_SET(g_Socket, &writeSet);

            struct timeval timeout;
            timeout.tv_sec = 1;
            timeout.tv_usec = 0;

            result = select(0, NULL, &writeSet, NULL, &timeout);
            if (result <= 0) {
                closesocket(g_Socket);
                g_Socket = INVALID_SOCKET;
                return false;
            }

            // Check if connection succeeded
            int optval;
            int optlen = sizeof(optval);
            if (getsockopt(g_Socket, SOL_SOCKET, SO_ERROR, (char*)&optval, &optlen) == 0 && optval != 0) {
                closesocket(g_Socket);
                g_Socket = INVALID_SOCKET;
                return false;
            }
        } else {
            closesocket(g_Socket);
            g_Socket = INVALID_SOCKET;
            return false;
        }
    }

    // Set back to blocking mode
    mode = 0;
    ioctlsocket(g_Socket, FIONBIO, &mode);

    // Set TCP_NODELAY for low latency
    int flag = 1;
    setsockopt(g_Socket, IPPROTO_TCP, TCP_NODELAY, (char*)&flag, sizeof(flag));

    // Set small send buffer
    int bufSize = 8192;
    setsockopt(g_Socket, SOL_SOCKET, SO_SNDBUF, (char*)&bufSize, sizeof(bufSize));

    return true;
}

// Send tick data
bool SendTick(const TickMessage& msg) {
    if (g_Socket == INVALID_SOCKET) {
        return false;
    }

    int result = send(g_Socket, (const char*)&msg, sizeof(msg), 0);

    if (result == SOCKET_ERROR) {
        int error = WSAGetLastError();
        if (error != WSAEWOULDBLOCK) {
            // Connection lost
            closesocket(g_Socket);
            g_Socket = INVALID_SOCKET;
            return false;
        }
    }

    return true;
}

// Disconnect
void Disconnect() {
    if (g_Socket != INVALID_SOCKET) {
        closesocket(g_Socket);
        g_Socket = INVALID_SOCKET;
    }
}

SCSFExport scsf_PhaseShifterStream(SCStudyInterfaceRef sc) {
    // Subgraph for visual indicator
    SCSubgraphRef Connected = sc.Subgraph[0];

    // Inputs
    SCInputRef ServerPort = sc.Input[0];
    SCInputRef LogInterval = sc.Input[1];

    // Persistent variables
    int& IsConnected = sc.GetPersistentInt(0);
    int& ReconnectAttempts = sc.GetPersistentInt(1);
    int64_t& TicksSent = sc.GetPersistentInt64(2);
    int64_t& LastLogTicks = sc.GetPersistentInt64(3);
    int& SymbolID_int = sc.GetPersistentInt(4);
    double& LastReconnectTime = sc.GetPersistentDouble(0);
    double& LastTickTime = sc.GetPersistentDouble(1);
    double& LastPrice = sc.GetPersistentDouble(2);
    double& LastVolume = sc.GetPersistentDouble(3);

    // Convert SymbolID to uint32_t for use (Sierra Chart only provides int storage)
    uint32_t& SymbolID = reinterpret_cast<uint32_t&>(SymbolID_int);

    // Study configuration
    if (sc.SetDefaults) {
        sc.GraphName = "PhaseShifter Stream";
        sc.StudyDescription = "Streams tick data to PhaseShifter Rust server via TCP";
        sc.AutoLoop = 1;  // Process every bar update
        sc.GraphRegion = 0;
        sc.FreeDLL = 0;  // Keep DLL loaded for persistent socket
        sc.UpdateAlways = 1;  // Update even when chart is not visible

        Connected.Name = "Connected";
        Connected.DrawStyle = DRAWSTYLE_BACKGROUND;
        Connected.PrimaryColor = RGB(0, 100, 0);
        Connected.SecondaryColor = RGB(100, 0, 0);
        Connected.SecondaryColorUsed = 1;
        Connected.DrawZeros = 0;

        ServerPort.Name = "Server Port";
        ServerPort.SetInt(9000);
        ServerPort.SetIntLimits(1024, 65535);

        LogInterval.Name = "Log Interval (ticks)";
        LogInterval.SetInt(10000);
        LogInterval.SetIntLimits(1000, 1000000);

        return;
    }

    // Handle study removal - close socket
    if (sc.LastCallToFunction) {
        Disconnect();
        IsConnected = 0;
        sc.AddMessageToLog("PhaseShifter: Study removed, disconnected", 0);
        return;
    }

    // Calculate symbol ID from chart symbol
    // IMPORTANT: Recalculate every time to ensure correct symbol ID per chart instance
    SCString fullSymbol;
    fullSymbol.Format("%s", sc.Symbol.GetChars());
    uint32_t currentSymbolID = HashSymbol(fullSymbol.GetChars());
    
    // Log on first run or if symbol changed
    if (SymbolID != currentSymbolID) {
        SymbolID = currentSymbolID;
        SCString msg;
        msg.Format("PhaseShifter: Symbol %s (ID: %u)",
                   fullSymbol.GetChars(), SymbolID);
        sc.AddMessageToLog(msg, 0);
    }

    // Current time as double (for reconnect timing)
    double currentTime = sc.CurrentSystemDateTime.GetAsDouble();

    // Try to connect if not connected
    if (!IsConnected || g_Socket == INVALID_SOCKET) {
        IsConnected = 0;

        // Reconnect every 5 seconds
        double timeSinceLastAttempt = (LastReconnectTime == 0) ? 999.0 :
            (currentTime - LastReconnectTime) * 86400.0;  // Days to seconds

        if (timeSinceLastAttempt >= 5.0) {
            LastReconnectTime = currentTime;
            ReconnectAttempts++;

            if (ConnectToServer(ServerPort.GetInt(), sc)) {
                IsConnected = 1;
                ReconnectAttempts = 0;

                SCString msg;
                msg.Format("PhaseShifter: Connected to 127.0.0.1:%d", ServerPort.GetInt());
                sc.AddMessageToLog(msg, 0);
            } else {
                if (ReconnectAttempts % 12 == 1) {  // Log every minute
                    SCString msg;
                    msg.Format("PhaseShifter: Waiting for server on port %d (attempt %d)...",
                               ServerPort.GetInt(), ReconnectAttempts);
                    sc.AddMessageToLog(msg, 0);
                }
            }
        }

        Connected.Data[sc.ArraySize - 1] = 0;  // Red when disconnected
        return;
    }

    // Update visual indicator - green when connected
    Connected.Data[sc.ArraySize - 1] = 1;

    // Get the latest bar/tick
    int lastIndex = sc.ArraySize - 1;
    if (lastIndex < 0) {
        return;
    }

    // On full recalculation, initialize tracking variables
    if (sc.IsFullRecalculation) {
        LastTickTime = sc.BaseDateTimeIn[lastIndex].GetAsDouble();
        LastPrice = sc.Close[lastIndex];
        LastVolume = sc.Volume[lastIndex];
        
        SCString msg;
        msg.Format("PhaseShifter: Study initialized, latest bar time=%f, price=%.2f", 
                   LastTickTime, sc.Close[lastIndex]);
        sc.AddMessageToLog(msg, 0);
        return;
    }

    // Get current tick data
    double tickTime = sc.BaseDateTimeIn[lastIndex].GetAsDouble();
    double currentPrice = sc.Close[lastIndex];
    double currentVolume = sc.Volume[lastIndex];
    
    // Check if we have new data (price changed OR volume changed OR time advanced)
    bool priceChanged = (currentPrice != LastPrice);
    bool volumeChanged = (currentVolume != LastVolume);
    bool timeAdvanced = (tickTime > LastTickTime);
    
    if (!priceChanged && !volumeChanged && !timeAdvanced) {
        // No new data
        return;
    }
    
    // Update tracking variables
    LastTickTime = tickTime;
    LastPrice = currentPrice;
    LastVolume = currentVolume;
    
    // Debug: Log to verify study is working
    if (TicksSent == 0) {
        SCString msg;
        msg.Format("PhaseShifter: First live tick, price=%.2f", sc.Close[lastIndex]);
        sc.AddMessageToLog(msg, 0);
    }

    // Build tick message
    TickMessage tickMsg;
    tickMsg.msg_type = 1;  // Tick message
    tickMsg.reserved[0] = 0;
    tickMsg.reserved[1] = 0;
    tickMsg.reserved[2] = 0;
    tickMsg.symbol_id = SymbolID;
    tickMsg.price = sc.Close[lastIndex];
    tickMsg.volume = sc.Volume[lastIndex];

    // Use exchange time from Sierra Chart (sc.BaseDateTimeIn)
    // This matches SCID file timestamps for accurate bar building and verification
    SCDateTime exchangeTime = sc.BaseDateTimeIn[lastIndex];
    double days_since_unix = exchangeTime.GetAsDouble() - 25569.0;
    tickMsg.timestamp_us = (int64_t)(days_since_unix * 86400.0 * 1000000.0);

    // Send the message
    if (!SendTick(tickMsg)) {
        IsConnected = 0;
        sc.AddMessageToLog("PhaseShifter: Connection lost, will reconnect", 1);
        return;
    }

    TicksSent++;

    // Periodic logging
    int logInterval = LogInterval.GetInt();
    if (TicksSent - LastLogTicks >= logInterval) {
        LastLogTicks = TicksSent;
        SCString logMsg;
        logMsg.Format("PhaseShifter: Sent %lld ticks, last price: %.2f",
                      TicksSent, tickMsg.price);
        sc.AddMessageToLog(logMsg, 0);
    }
}
