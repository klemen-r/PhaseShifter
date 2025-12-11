import asyncio
import json
import time

import websockets

SEND_EVERY = 1.0  # seconds


async def sender(ws):
    """Send periodic messages to one client."""
    try:
        while True:
            payload = {
                "type": "tick",
                "ts": time.time(),
                "msg": "hello from python server",
            }
            await ws.send(json.dumps(payload))
            await asyncio.sleep(SEND_EVERY)
    except websockets.exceptions.ConnectionClosed:
        pass


async def receiver(ws):
    """Receive messages from one client."""
    try:
        async for msg in ws:
            print("Received from client:", msg)
            if msg == "ping":
                payload = {
                    "msg": "pong",
                }
            await ws.send(json.dumps(payload))
    except websockets.exceptions.ConnectionClosed:
        pass


async def handle(ws):
    print("Client connected")

    send_task = asyncio.create_task(sender(ws))
    recv_task = asyncio.create_task(receiver(ws))

    done, pending = await asyncio.wait(
        {send_task, recv_task}, return_when=asyncio.FIRST_COMPLETED
    )

    for task in pending:
        task.cancel()

    print("Client disconnected")


async def main():
    print("WebSocket server running on ws://localhost:8000")
    async with websockets.serve(handle, "localhost", 8000):
        await asyncio.Future()  # run forever


if __name__ == "__main__":
    asyncio.run(main())
