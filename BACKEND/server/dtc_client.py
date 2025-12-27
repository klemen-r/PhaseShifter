"""
DTC Protocol Client for Sierra Chart.

Connects to Sierra Chart's DTC server and streams real-time market data.
Handles connection lifecycle, heartbeats, and market data subscriptions.
"""

import asyncio
import logging
import time
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum, auto
from typing import Any, Callable, Optional

from dtc_protocol import (
    HEARTBEAT_INTERVAL_SECONDS,
    DTCMessageType,
    EncodingEnum,
    EncodingRequest,
    EncodingResponse,
    Heartbeat,
    LogonRequest,
    LogonResponse,
    LogonStatusEnum,
    MarketDataReject,
    MarketDataRequest,
    MarketDataSnapshot,
    MarketDataUpdateBidAsk,
    MarketDataUpdateBidAskCompact,
    MarketDataUpdateSessionHigh,
    MarketDataUpdateSessionLow,
    MarketDataUpdateSessionVolume,
    MarketDataUpdateTrade,
    MarketDataUpdateTradeCompact,
    MessageHeader,
    RequestActionEnum,
    parse_message,
)

logger = logging.getLogger(__name__)


class ConnectionState(Enum):
    DISCONNECTED = auto()
    CONNECTING = auto()
    ENCODING = auto()
    LOGGING_IN = auto()
    CONNECTED = auto()
    RECONNECTING = auto()


@dataclass
class SymbolSubscription:
    """Tracks a market data subscription."""

    symbol: str
    symbol_id: int = 0
    subscribed: bool = False
    last_price: float = 0.0
    last_volume: float = 0.0
    bid_price: float = 0.0
    ask_price: float = 0.0
    session_high: float = 0.0
    session_low: float = 0.0
    session_volume: float = 0.0
    last_update: float = 0.0


@dataclass
class Tick:
    """Represents a single trade tick."""

    symbol: str
    price: float
    volume: float
    timestamp: float  # Unix timestamp
    at_bid_or_ask: int = 0  # 0=unknown, 1=at_bid, 2=at_ask
    bid_price: float = 0.0
    ask_price: float = 0.0


# Type aliases for callbacks
TickCallback = Callable[[Tick], None]
ConnectionCallback = Callable[[ConnectionState], None]
ErrorCallback = Callable[[str], None]


class DTCClient:
    """
    Async DTC Protocol client for Sierra Chart market data.

    Usage:
        client = DTCClient("127.0.0.1", 11099)
        client.on_tick = my_tick_handler
        await client.connect()
        await client.subscribe("NQH26")
        # ... client streams ticks via callback
        await client.disconnect()
    """

    def __init__(
        self,
        host: str = "127.0.0.1",
        port: int = 11099,
        client_name: str = "PhaseShifter",
        heartbeat_interval: int = HEARTBEAT_INTERVAL_SECONDS,
        reconnect_delay: float = 5.0,
        max_reconnect_attempts: int = 10,
    ):
        self.host = host
        self.port = port
        self.client_name = client_name
        self.heartbeat_interval = heartbeat_interval
        self.reconnect_delay = reconnect_delay
        self.max_reconnect_attempts = max_reconnect_attempts

        # Connection state
        self._state = ConnectionState.DISCONNECTED
        self._reader: Optional[asyncio.StreamReader] = None
        self._writer: Optional[asyncio.StreamWriter] = None
        self._recv_buffer = bytearray()
        self._next_symbol_id = 1
        self._reconnect_attempts = 0
        self._should_reconnect = True

        # Subscriptions: symbol -> SymbolSubscription
        self._subscriptions: dict[str, SymbolSubscription] = {}
        # symbol_id -> symbol (reverse lookup)
        self._symbol_id_map: dict[int, str] = {}

        # Tasks
        self._recv_task: Optional[asyncio.Task] = None
        self._heartbeat_task: Optional[asyncio.Task] = None
        self._last_heartbeat_sent: float = 0
        self._last_heartbeat_recv: float = 0

        # Callbacks
        self.on_tick: Optional[TickCallback] = None
        self.on_connection_state: Optional[ConnectionCallback] = None
        self.on_error: Optional[ErrorCallback] = None

        # Connection event for waiting on connect
        self._connected_event = asyncio.Event()
        self._shutdown_event = asyncio.Event()

    @property
    def state(self) -> ConnectionState:
        return self._state

    @property
    def is_connected(self) -> bool:
        return self._state == ConnectionState.CONNECTED

    def _set_state(self, state: ConnectionState):
        """Update connection state and notify callback."""
        if self._state != state:
            old_state = self._state
            self._state = state
            logger.info(f"DTC state: {old_state.name} -> {state.name}")
            if self.on_connection_state:
                try:
                    self.on_connection_state(state)
                except Exception as e:
                    logger.error(f"Error in connection state callback: {e}")

    async def connect(self) -> bool:
        """
        Connect to the DTC server.

        Returns:
            True if connected successfully, False otherwise.
        """
        if self._state != ConnectionState.DISCONNECTED:
            logger.warning(f"Cannot connect: already in state {self._state.name}")
            return False

        self._should_reconnect = True
        self._shutdown_event.clear()
        self._connected_event.clear()

        return await self._do_connect()

    async def _do_connect(self) -> bool:
        """Internal connection logic."""
        self._set_state(ConnectionState.CONNECTING)

        try:
            logger.info(f"Connecting to DTC server at {self.host}:{self.port}")
            self._reader, self._writer = await asyncio.wait_for(
                asyncio.open_connection(self.host, self.port),
                timeout=10.0,
            )
            logger.info("TCP connection established")

            # Start receive task first to handle responses
            self._recv_task = asyncio.create_task(self._receive_loop())

            # Send encoding request
            self._set_state(ConnectionState.ENCODING)
            await self._send(EncodingRequest().pack())

            # Wait for connected state with timeout
            try:
                await asyncio.wait_for(self._connected_event.wait(), timeout=10.0)
            except asyncio.TimeoutError:
                logger.error("Timeout waiting for logon response")
                await self._cleanup()
                return False

            # Start heartbeat task
            self._heartbeat_task = asyncio.create_task(self._heartbeat_loop())

            self._reconnect_attempts = 0
            logger.info("DTC connection fully established")
            return True

        except asyncio.TimeoutError:
            logger.error(f"Connection timeout to {self.host}:{self.port}")
            self._emit_error(f"Connection timeout to {self.host}:{self.port}")
            await self._cleanup()
            return False
        except OSError as e:
            logger.error(f"Connection failed: {e}")
            self._emit_error(f"Connection failed: {e}")
            await self._cleanup()
            return False

    async def disconnect(self):
        """Disconnect from the DTC server."""
        logger.info("Disconnecting from DTC server")
        self._should_reconnect = False
        self._shutdown_event.set()
        await self._cleanup()

    async def _cleanup(self):
        """Clean up connection resources."""
        # Cancel tasks
        if self._heartbeat_task and not self._heartbeat_task.done():
            self._heartbeat_task.cancel()
            try:
                await self._heartbeat_task
            except asyncio.CancelledError:
                pass
            self._heartbeat_task = None

        if self._recv_task and not self._recv_task.done():
            self._recv_task.cancel()
            try:
                await self._recv_task
            except asyncio.CancelledError:
                pass
            self._recv_task = None

        # Close connection
        if self._writer:
            try:
                self._writer.close()
                await self._writer.wait_closed()
            except Exception:
                pass
            self._writer = None
            self._reader = None

        self._recv_buffer.clear()
        self._connected_event.clear()
        self._set_state(ConnectionState.DISCONNECTED)

    async def _send(self, data: bytes):
        """Send data to the server."""
        if self._writer is None:
            raise ConnectionError("Not connected")

        self._writer.write(data)
        await self._writer.drain()

    async def _receive_loop(self):
        """Main receive loop - reads and parses messages."""
        try:
            while not self._shutdown_event.is_set():
                if self._reader is None:
                    break

                try:
                    data = await asyncio.wait_for(
                        self._reader.read(8192),
                        timeout=self.heartbeat_interval * 2,
                    )
                except asyncio.TimeoutError:
                    logger.warning("No data received for heartbeat interval")
                    continue

                if not data:
                    logger.warning("Connection closed by server")
                    break

                self._recv_buffer.extend(data)
                await self._process_buffer()

        except asyncio.CancelledError:
            raise
        except Exception as e:
            logger.error(f"Receive loop error: {e}")
            self._emit_error(f"Receive error: {e}")

        # Handle disconnection
        if self._should_reconnect and not self._shutdown_event.is_set():
            asyncio.create_task(self._handle_reconnect())

    async def _process_buffer(self):
        """Process all complete messages in the receive buffer."""
        while len(self._recv_buffer) >= MessageHeader.SIZE:
            msg, consumed = parse_message(bytes(self._recv_buffer))
            if consumed == 0:
                break  # Incomplete message

            del self._recv_buffer[:consumed]
            if msg:
                await self._handle_message(msg)

    async def _handle_message(self, msg: Any):
        """Route parsed message to appropriate handler."""
        if isinstance(msg, EncodingResponse):
            await self._handle_encoding_response(msg)
        elif isinstance(msg, LogonResponse):
            await self._handle_logon_response(msg)
        elif isinstance(msg, Heartbeat):
            self._handle_heartbeat(msg)
        elif isinstance(msg, MarketDataReject):
            self._handle_market_data_reject(msg)
        elif isinstance(msg, MarketDataSnapshot):
            self._handle_market_data_snapshot(msg)
        elif isinstance(msg, (MarketDataUpdateTrade, MarketDataUpdateTradeCompact)):
            self._handle_trade_update(msg)
        elif isinstance(msg, (MarketDataUpdateBidAsk, MarketDataUpdateBidAskCompact)):
            self._handle_bid_ask_update(msg)
        elif isinstance(msg, MarketDataUpdateSessionHigh):
            self._handle_session_high(msg)
        elif isinstance(msg, MarketDataUpdateSessionLow):
            self._handle_session_low(msg)
        elif isinstance(msg, MarketDataUpdateSessionVolume):
            self._handle_session_volume(msg)
        elif isinstance(msg, dict):
            # Unknown message type
            msg_type = msg.get("type", "unknown")
            logger.debug(f"Unhandled message type: {msg_type}")

    async def _handle_encoding_response(self, msg: EncodingResponse):
        """Handle encoding response - proceed to logon."""
        logger.info(
            f"Encoding accepted: version={msg.protocol_version}, encoding={msg.encoding}"
        )

        # Send logon request
        self._set_state(ConnectionState.LOGGING_IN)
        logon = LogonRequest(
            client_name=self.client_name,
            heartbeat_interval=self.heartbeat_interval,
        )
        await self._send(logon.pack())

    async def _handle_logon_response(self, msg: LogonResponse):
        """Handle logon response."""
        if msg.result == LogonStatusEnum.SUCCESS:
            logger.info(f"Logged in successfully. Server: {msg.server_name}")
            self._set_state(ConnectionState.CONNECTED)
            self._connected_event.set()
            self._last_heartbeat_recv = time.time()

            # Resubscribe to any pending symbols
            for symbol in list(self._subscriptions.keys()):
                await self._send_subscribe(symbol)
        else:
            logger.error(f"Logon failed: {msg.result_text}")
            self._emit_error(f"Logon failed: {msg.result_text}")
            await self._cleanup()

    def _handle_heartbeat(self, msg: Heartbeat):
        """Handle heartbeat from server."""
        self._last_heartbeat_recv = time.time()
        if msg.num_dropped_messages > 0:
            logger.warning(
                f"Server reports {msg.num_dropped_messages} dropped messages"
            )

    def _handle_market_data_reject(self, msg: MarketDataReject):
        """Handle market data request rejection."""
        symbol = self._symbol_id_map.get(msg.symbol_id, f"id:{msg.symbol_id}")
        logger.error(f"Market data rejected for {symbol}: {msg.reject_text}")
        self._emit_error(f"Market data rejected for {symbol}: {msg.reject_text}")

        # Mark subscription as failed
        if msg.symbol_id in self._symbol_id_map:
            symbol = self._symbol_id_map[msg.symbol_id]
            if symbol in self._subscriptions:
                self._subscriptions[symbol].subscribed = False

    def _handle_market_data_snapshot(self, msg: MarketDataSnapshot):
        """Handle initial market data snapshot."""
        symbol = self._symbol_id_map.get(msg.symbol_id)
        if not symbol:
            return

        sub = self._subscriptions.get(symbol)
        if sub:
            sub.subscribed = True
            sub.last_price = msg.last_trade_price
            sub.bid_price = msg.bid_price
            sub.ask_price = msg.ask_price
            sub.session_high = msg.session_high_price
            sub.session_low = msg.session_low_price
            sub.session_volume = msg.session_volume
            sub.last_update = time.time()

            logger.info(
                f"Snapshot for {symbol}: last={msg.last_trade_price:.2f}, "
                f"bid={msg.bid_price:.2f}, ask={msg.ask_price:.2f}"
            )

            # Emit initial tick
            if self.on_tick and msg.last_trade_price > 0:
                tick = Tick(
                    symbol=symbol,
                    price=msg.last_trade_price,
                    volume=msg.last_trade_volume,
                    timestamp=msg.last_trade_date_time
                    if msg.last_trade_date_time > 0
                    else time.time(),
                    bid_price=msg.bid_price,
                    ask_price=msg.ask_price,
                )
                self._emit_tick(tick)

    def _handle_trade_update(
        self, msg: MarketDataUpdateTrade | MarketDataUpdateTradeCompact
    ):
        """Handle real-time trade update."""
        symbol = self._symbol_id_map.get(msg.symbol_id)
        if not symbol:
            return

        sub = self._subscriptions.get(symbol)
        if sub:
            sub.last_price = msg.price
            sub.last_volume = msg.volume
            sub.last_update = time.time()

        if self.on_tick:
            tick = Tick(
                symbol=symbol,
                price=msg.price,
                volume=msg.volume,
                timestamp=msg.date_time if msg.date_time > 0 else time.time(),
                at_bid_or_ask=msg.at_bid_or_ask,
                bid_price=sub.bid_price if sub else 0.0,
                ask_price=sub.ask_price if sub else 0.0,
            )
            self._emit_tick(tick)

    def _handle_bid_ask_update(
        self, msg: MarketDataUpdateBidAsk | MarketDataUpdateBidAskCompact
    ):
        """Handle bid/ask update."""
        symbol = self._symbol_id_map.get(msg.symbol_id)
        if not symbol:
            return

        sub = self._subscriptions.get(symbol)
        if sub:
            sub.bid_price = msg.bid_price
            sub.ask_price = msg.ask_price
            sub.last_update = time.time()

    def _handle_session_high(self, msg: MarketDataUpdateSessionHigh):
        """Handle session high update."""
        symbol = self._symbol_id_map.get(msg.symbol_id)
        if symbol and symbol in self._subscriptions:
            self._subscriptions[symbol].session_high = msg.price

    def _handle_session_low(self, msg: MarketDataUpdateSessionLow):
        """Handle session low update."""
        symbol = self._symbol_id_map.get(msg.symbol_id)
        if symbol and symbol in self._subscriptions:
            self._subscriptions[symbol].session_low = msg.price

    def _handle_session_volume(self, msg: MarketDataUpdateSessionVolume):
        """Handle session volume update."""
        symbol = self._symbol_id_map.get(msg.symbol_id)
        if symbol and symbol in self._subscriptions:
            self._subscriptions[symbol].session_volume = msg.volume

    def _emit_tick(self, tick: Tick):
        """Emit tick to callback."""
        if self.on_tick:
            try:
                self.on_tick(tick)
            except Exception as e:
                logger.error(f"Error in tick callback: {e}")

    def _emit_error(self, message: str):
        """Emit error to callback."""
        if self.on_error:
            try:
                self.on_error(message)
            except Exception as e:
                logger.error(f"Error in error callback: {e}")

    async def _heartbeat_loop(self):
        """Send periodic heartbeats."""
        try:
            while not self._shutdown_event.is_set():
                await asyncio.sleep(self.heartbeat_interval)

                if self._state != ConnectionState.CONNECTED:
                    continue

                # Send heartbeat
                try:
                    heartbeat = Heartbeat(current_date_time=int(time.time()))
                    await self._send(heartbeat.pack())
                    self._last_heartbeat_sent = time.time()
                except Exception as e:
                    logger.error(f"Failed to send heartbeat: {e}")
                    break

                # Check if we've received a heartbeat recently
                if (
                    time.time() - self._last_heartbeat_recv
                    > self.heartbeat_interval * 3
                ):
                    logger.warning("No heartbeat from server, connection may be stale")

        except asyncio.CancelledError:
            raise

    async def _handle_reconnect(self):
        """Handle reconnection after disconnect."""
        if not self._should_reconnect:
            return

        self._set_state(ConnectionState.RECONNECTING)
        self._reconnect_attempts += 1

        if self._reconnect_attempts > self.max_reconnect_attempts:
            logger.error(
                f"Max reconnect attempts ({self.max_reconnect_attempts}) exceeded"
            )
            self._emit_error("Max reconnect attempts exceeded")
            await self._cleanup()
            return

        delay = min(self.reconnect_delay * self._reconnect_attempts, 60.0)
        logger.info(
            f"Reconnecting in {delay:.1f}s (attempt {self._reconnect_attempts})"
        )

        await asyncio.sleep(delay)

        if self._should_reconnect and not self._shutdown_event.is_set():
            await self._cleanup()
            await self._do_connect()

    async def subscribe(self, symbol: str) -> bool:
        """
        Subscribe to market data for a symbol.

        Args:
            symbol: Symbol to subscribe to (e.g., "NQH26", "ESH26")

        Returns:
            True if subscription request was sent successfully.
        """
        if symbol in self._subscriptions:
            logger.debug(f"Already subscribed to {symbol}")
            return True

        # Create subscription entry
        symbol_id = self._next_symbol_id
        self._next_symbol_id += 1

        self._subscriptions[symbol] = SymbolSubscription(
            symbol=symbol,
            symbol_id=symbol_id,
        )
        self._symbol_id_map[symbol_id] = symbol

        if self._state == ConnectionState.CONNECTED:
            return await self._send_subscribe(symbol)
        else:
            logger.info(f"Queued subscription for {symbol} (not connected)")
            return True

    async def _send_subscribe(self, symbol: str) -> bool:
        """Send subscription request to server."""
        sub = self._subscriptions.get(symbol)
        if not sub:
            return False

        try:
            request = MarketDataRequest(
                request_action=RequestActionEnum.SUBSCRIBE,
                symbol_id=sub.symbol_id,
                symbol=symbol,
                exchange="",  # Sierra Chart doesn't require exchange
            )
            await self._send(request.pack())
            logger.info(f"Sent subscription request for {symbol} (id={sub.symbol_id})")
            return True
        except Exception as e:
            logger.error(f"Failed to subscribe to {symbol}: {e}")
            return False

    async def unsubscribe(self, symbol: str) -> bool:
        """
        Unsubscribe from market data for a symbol.

        Args:
            symbol: Symbol to unsubscribe from.

        Returns:
            True if unsubscription was successful.
        """
        sub = self._subscriptions.get(symbol)
        if not sub:
            return True

        if self._state == ConnectionState.CONNECTED:
            try:
                request = MarketDataRequest(
                    request_action=RequestActionEnum.UNSUBSCRIBE,
                    symbol_id=sub.symbol_id,
                    symbol=symbol,
                )
                await self._send(request.pack())
                logger.info(f"Sent unsubscribe request for {symbol}")
            except Exception as e:
                logger.error(f"Failed to unsubscribe from {symbol}: {e}")

        # Clean up
        del self._subscriptions[symbol]
        del self._symbol_id_map[sub.symbol_id]
        return True

    def get_subscription(self, symbol: str) -> Optional[SymbolSubscription]:
        """Get subscription info for a symbol."""
        return self._subscriptions.get(symbol)

    def get_subscribed_symbols(self) -> list[str]:
        """Get list of subscribed symbols."""
        return list(self._subscriptions.keys())

    def get_status(self) -> dict:
        """Get client status."""
        return {
            "state": self._state.name,
            "host": self.host,
            "port": self.port,
            "subscriptions": [
                {
                    "symbol": sub.symbol,
                    "symbol_id": sub.symbol_id,
                    "subscribed": sub.subscribed,
                    "last_price": sub.last_price,
                    "bid": sub.bid_price,
                    "ask": sub.ask_price,
                }
                for sub in self._subscriptions.values()
            ],
            "last_heartbeat_sent": self._last_heartbeat_sent,
            "last_heartbeat_recv": self._last_heartbeat_recv,
            "reconnect_attempts": self._reconnect_attempts,
        }
