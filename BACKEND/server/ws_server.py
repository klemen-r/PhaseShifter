"""
WebSocket server with client and subscription management.
"""

import asyncio
import json
import uuid
from dataclasses import dataclass, field
from datetime import datetime
from typing import Callable, Optional

import websockets
from websockets.server import WebSocketServerProtocol


@dataclass
class Client:
    id: str
    websocket: WebSocketServerProtocol
    subscriptions: set[str] = field(default_factory=set)
    auto_clusters: set[str] = field(
        default_factory=set
    )  # Tickers with auto-clusters enabled
    connected_at: datetime = field(default_factory=datetime.utcnow)

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "subscriptions": list(self.subscriptions),
            "auto_clusters": list(self.auto_clusters),
            "connected_at": self.connected_at.isoformat(),
        }


class WebSocketServer:
    def __init__(self, host: str = "localhost", port: int = 8000):
        self.host = host
        self.port = port
        self.clients: dict[str, Client] = {}
        self._server: Optional[websockets.WebSocketServer] = None
        self._started_at: Optional[datetime] = None

        # Callbacks for external components
        self.on_subscribe: Optional[Callable[[str, str], None]] = (
            None  # (client_id, ticker)
        )
        self.on_unsubscribe: Optional[Callable[[str, str], None]] = None
        self.on_get_clusters: Optional[Callable[[str, str], asyncio.Future]] = (
            None  # returns data
        )

    async def start(self):
        """Start the WebSocket server."""
        self._server = await websockets.serve(
            self._handle_client,
            self.host,
            self.port,
        )
        self._started_at = datetime.utcnow()
        print(f"WebSocket server running on ws://{self.host}:{self.port}")

    async def stop(self):
        """Stop the WebSocket server."""
        if self._server:
            self._server.close()
            await self._server.wait_closed()
            print("WebSocket server stopped")

    async def _handle_client(self, websocket: WebSocketServerProtocol):
        """Handle a new client connection."""
        client_id = f"client_{uuid.uuid4().hex[:8]}"
        client = Client(id=client_id, websocket=websocket)
        self.clients[client_id] = client

        print(f"[+] Client connected: {client_id}")

        try:
            async for message in websocket:
                await self._handle_message(client, message)
        except websockets.ConnectionClosed:
            pass
        finally:
            # Clean up subscriptions
            for ticker in list(client.subscriptions):
                if self.on_unsubscribe:
                    self.on_unsubscribe(client_id, ticker)
            del self.clients[client_id]
            print(f"[-] Client disconnected: {client_id}")

    async def _handle_message(self, client: Client, raw_message: str):
        """Route incoming messages to appropriate handlers."""
        try:
            msg = json.loads(raw_message)
        except json.JSONDecodeError:
            await self._send_error(client, "Invalid JSON")
            return

        msg_type = msg.get("type")

        if msg_type == "ping":
            print(f"    {client.id} ping")
            await self._send(client, {"type": "pong"})

        elif msg_type == "subscribe":
            ticker = msg.get("ticker")
            if not ticker:
                await self._send_error(client, "Missing ticker")
                return
            client.subscriptions.add(ticker)
            if self.on_subscribe:
                self.on_subscribe(client.id, ticker)
            await self._send(client, {"type": "subscribed", "ticker": ticker})
            print(f"    {client.id} subscribed to {ticker}")

        elif msg_type == "unsubscribe":
            ticker = msg.get("ticker")
            if ticker and ticker in client.subscriptions:
                client.subscriptions.discard(ticker)
                if self.on_unsubscribe:
                    self.on_unsubscribe(client.id, ticker)
                await self._send(client, {"type": "unsubscribed", "ticker": ticker})
                print(f"    {client.id} unsubscribed from {ticker}")

        elif msg_type == "get_clusters":
            ticker = msg.get("ticker")
            if not ticker:
                await self._send_error(client, "Missing ticker")
                return
            if self.on_get_clusters:
                try:
                    data = await self.on_get_clusters(client.id, ticker)
                    await self._send(
                        client,
                        {
                            "type": "clusters",
                            "ticker": ticker,
                            "data": data,
                        },
                    )
                except Exception as e:
                    await self._send_error(client, f"Failed to get clusters: {e}")
            else:
                await self._send_error(client, "Cluster data not available")

        elif msg_type == "enable_auto_clusters":
            ticker = msg.get("ticker")
            if not ticker:
                await self._send_error(client, "Missing ticker")
                return
            client.auto_clusters.add(ticker)
            await self._send(
                client, {"type": "auto_clusters_enabled", "ticker": ticker}
            )
            print(f"    {client.id} enabled auto-clusters for {ticker}")

        elif msg_type == "disable_auto_clusters":
            ticker = msg.get("ticker")
            if ticker and ticker in client.auto_clusters:
                client.auto_clusters.discard(ticker)
                await self._send(
                    client, {"type": "auto_clusters_disabled", "ticker": ticker}
                )
                print(f"    {client.id} disabled auto-clusters for {ticker}")

        else:
            await self._send_error(client, f"Unknown message type: {msg_type}")

    async def _send(self, client: Client, data: dict):
        """Send JSON message to a client."""
        try:
            await client.websocket.send(json.dumps(data))
        except websockets.ConnectionClosed:
            pass

    async def _send_error(self, client: Client, message: str):
        """Send error message to a client."""
        await self._send(client, {"type": "error", "message": message})

    # Public methods for external components

    async def broadcast(self, data: dict):
        """Broadcast message to all connected clients."""
        message = json.dumps(data)
        for client in list(self.clients.values()):
            try:
                await client.websocket.send(message)
            except websockets.ConnectionClosed:
                pass

    async def broadcast_to_ticker(self, ticker: str, data: dict):
        """Broadcast message to all clients subscribed to a ticker."""
        message = json.dumps(data)
        for client in list(self.clients.values()):
            if ticker in client.subscriptions:
                try:
                    await client.websocket.send(message)
                except websockets.ConnectionClosed:
                    pass

    async def send_to_client(self, client_id: str, data: dict):
        """Send message to a specific client."""
        client = self.clients.get(client_id)
        if client:
            await self._send(client, data)

    async def kick_client(self, client_id: str):
        """Disconnect a client."""
        client = self.clients.get(client_id)
        if client:
            await client.websocket.close(1000, "Kicked by server")

    def get_all_subscribed_tickers(self) -> set[str]:
        """Get all tickers that have at least one subscriber."""
        tickers = set()
        for client in self.clients.values():
            tickers.update(client.subscriptions)
        return tickers

    def get_subscribers(self, ticker: str) -> list[str]:
        """Get list of client IDs subscribed to a ticker."""
        return [
            client.id
            for client in self.clients.values()
            if ticker in client.subscriptions
        ]

    def get_auto_cluster_subscribers(self, ticker: str) -> list[str]:
        """Get list of client IDs with auto-clusters enabled for a ticker."""
        return [
            client.id
            for client in self.clients.values()
            if ticker in client.auto_clusters
        ]

    async def broadcast_to_auto_cluster_clients(self, ticker: str, data: dict):
        """Broadcast message only to clients with auto-clusters enabled for ticker."""
        message = json.dumps(data)
        for client in list(self.clients.values()):
            if ticker in client.auto_clusters:
                try:
                    await client.websocket.send(message)
                except websockets.ConnectionClosed:
                    pass

    def get_status(self) -> dict:
        """Get server status."""
        uptime = None
        if self._started_at:
            uptime = (datetime.utcnow() - self._started_at).total_seconds()

        return {
            "host": self.host,
            "port": self.port,
            "clients": len(self.clients),
            "uptime_seconds": uptime,
            "started_at": self._started_at.isoformat() if self._started_at else None,
        }

    def list_clients(self) -> list[dict]:
        """List all connected clients."""
        return [client.to_dict() for client in self.clients.values()]

    def get_subscriptions_summary(self) -> dict[str, list[str]]:
        """Get mapping of ticker -> list of client IDs."""
        summary: dict[str, list[str]] = {}
        for client in self.clients.values():
            for ticker in client.subscriptions:
                if ticker not in summary:
                    summary[ticker] = []
                summary[ticker].append(client.id)
        return summary

    async def unsubscribe_all_from_ticker(self, ticker: str) -> int:
        """Forcefully unsubscribe all clients from a ticker. Returns count of affected clients."""
        count = 0
        for client in list(self.clients.values()):
            if ticker in client.subscriptions:
                client.subscriptions.discard(ticker)
                client.auto_clusters.discard(ticker)
                if self.on_unsubscribe:
                    self.on_unsubscribe(client.id, ticker)
                await self._send(client, {"type": "unsubscribed", "ticker": ticker})
                count += 1
        return count

    async def unsubscribe_all_tickers(self) -> dict[str, int]:
        """Forcefully unsubscribe all clients from all tickers. Returns count per ticker."""
        results = {}
        all_tickers = self.get_all_subscribed_tickers()
        for ticker in all_tickers:
            count = await self.unsubscribe_all_from_ticker(ticker)
            results[ticker] = count
        return results
