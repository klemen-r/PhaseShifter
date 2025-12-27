"""
DTC Protocol definitions and message structures.

Reference: https://dtcprotocol.org/
Sierra Chart uses the DTC Protocol for market data streaming.
"""

import struct
from dataclasses import dataclass, field
from enum import IntEnum
from typing import Any, Optional

# =============================================================================
# DTC Protocol Constants
# =============================================================================

DTC_VERSION = 8
HEARTBEAT_INTERVAL_SECONDS = 10


# Encoding types
class EncodingEnum(IntEnum):
    BINARY = 0
    BINARY_VLEN = 1  # Variable length binary (Sierra default)
    BINARY_WITH_TIMESTAMPS = 2
    JSON = 3
    JSON_COMPACT = 4
    PROTOCOL_BUFFERS = 5


# =============================================================================
# DTC Message Types
# =============================================================================


class DTCMessageType(IntEnum):
    # Session messages
    LOGON_REQUEST = 1
    LOGON_RESPONSE = 2
    HEARTBEAT = 3
    LOGOFF = 5
    ENCODING_REQUEST = 6
    ENCODING_RESPONSE = 7

    # Market data messages
    MARKET_DATA_REQUEST = 101
    MARKET_DATA_REJECT = 103
    MARKET_DATA_SNAPSHOT = 104
    MARKET_DATA_UPDATE_TRADE = 107
    MARKET_DATA_UPDATE_TRADE_COMPACT = 112
    MARKET_DATA_UPDATE_LAST_TRADE_SNAPSHOT = 134
    MARKET_DATA_UPDATE_TRADE_WITH_UNBUNDLED_INDICATOR = 137
    MARKET_DATA_UPDATE_BID_ASK = 108
    MARKET_DATA_UPDATE_BID_ASK_COMPACT = 117
    MARKET_DATA_UPDATE_BID_ASK_NO_TIMESTAMP = 143
    MARKET_DATA_UPDATE_SESSION_OPEN = 120
    MARKET_DATA_UPDATE_SESSION_HIGH = 114
    MARKET_DATA_UPDATE_SESSION_LOW = 115
    MARKET_DATA_UPDATE_SESSION_VOLUME = 113
    MARKET_DATA_UPDATE_OPEN_INTEREST = 124
    MARKET_DATA_UPDATE_SESSION_SETTLEMENT = 119
    MARKET_DATA_UPDATE_SESSION_NUM_TRADES = 135
    MARKET_DATA_UPDATE_TRADING_SESSION_DATE = 136

    # Symbol/security messages
    SECURITY_DEFINITION_FOR_SYMBOL_REQUEST = 506
    SECURITY_DEFINITION_RESPONSE = 507

    # Historical data
    HISTORICAL_PRICE_DATA_REQUEST = 800
    HISTORICAL_PRICE_DATA_RESPONSE_HEADER = 801
    HISTORICAL_PRICE_DATA_REJECT = 802
    HISTORICAL_PRICE_DATA_RECORD_RESPONSE = 803
    HISTORICAL_PRICE_DATA_TICK_RECORD_RESPONSE = 804
    HISTORICAL_PRICE_DATA_RECORD_RESPONSE_INT = 805
    HISTORICAL_PRICE_DATA_TICK_RECORD_RESPONSE_INT = 806


# Request action types
class RequestActionEnum(IntEnum):
    SUBSCRIBE = 1
    UNSUBSCRIBE = 2
    SNAPSHOT = 3


# Logon status
class LogonStatusEnum(IntEnum):
    SUCCESS = 1
    ERROR = 2
    ERROR_NO_RECONNECT = 3
    RECONNECT_NEW_ADDRESS = 4


# =============================================================================
# Message Structures (Binary Variable Length Encoding)
# =============================================================================


@dataclass
class MessageHeader:
    """2-byte header for binary variable length messages."""

    size: int  # uint16 - total message size including header
    type: int  # uint16 - message type

    FORMAT = "<HH"
    SIZE = 4

    def pack(self) -> bytes:
        return struct.pack(self.FORMAT, self.size, self.type)

    @classmethod
    def unpack(cls, data: bytes) -> "MessageHeader":
        size, msg_type = struct.unpack(cls.FORMAT, data[: cls.SIZE])
        return cls(size=size, type=msg_type)


@dataclass
class EncodingRequest:
    """Request encoding type from server."""

    protocol_version: int = DTC_VERSION
    encoding: int = EncodingEnum.BINARY_VLEN
    protocol_type: str = "DTC"

    TYPE = DTCMessageType.ENCODING_REQUEST

    def pack(self) -> bytes:
        # Fixed size: int32 version + int32 encoding + 4 char protocol
        body = struct.pack(
            "<ii4s",
            self.protocol_version,
            self.encoding,
            self.protocol_type.encode("ascii").ljust(4, b"\x00"),
        )
        header = MessageHeader(size=MessageHeader.SIZE + len(body), type=self.TYPE)
        return header.pack() + body


@dataclass
class EncodingResponse:
    """Server response to encoding request."""

    protocol_version: int = 0
    encoding: int = 0
    protocol_type: str = ""

    TYPE = DTCMessageType.ENCODING_RESPONSE

    @classmethod
    def unpack(cls, data: bytes) -> "EncodingResponse":
        # Skip header, parse body
        body = data[MessageHeader.SIZE :]
        if len(body) >= 12:
            version, encoding = struct.unpack("<ii", body[:8])
            protocol = body[8:12].rstrip(b"\x00").decode("ascii", errors="replace")
            return cls(
                protocol_version=version, encoding=encoding, protocol_type=protocol
            )
        return cls()


@dataclass
class LogonRequest:
    """Client logon request."""

    protocol_version: int = DTC_VERSION
    username: str = ""
    password: str = ""
    general_text_data: str = ""
    integer_1: int = 0
    integer_2: int = 0
    heartbeat_interval: int = HEARTBEAT_INTERVAL_SECONDS
    trade_mode: int = 0  # 0 = no trading
    trade_account: str = ""
    hardware_identifier: str = ""
    client_name: str = "PhaseShifter"

    TYPE = DTCMessageType.LOGON_REQUEST

    def pack(self) -> bytes:
        """Pack as variable length binary message."""
        # Format for variable length: int32 fields followed by null-terminated strings
        # We need to pack fixed fields then strings

        # Fixed fields: version(4) + ints(4*5) + heartbeat(4) = 28 bytes
        fixed = struct.pack(
            "<iiiiiii",
            self.protocol_version,
            self.integer_1,
            self.integer_2,
            self.heartbeat_interval,
            0,  # unused1
            0,  # unused2
            self.trade_mode,
        )

        # Variable length strings (null-terminated)
        def encode_str(s: str) -> bytes:
            encoded = s.encode("utf-8") + b"\x00"
            # Pad to 4-byte alignment
            padding = (4 - len(encoded) % 4) % 4
            return encoded + b"\x00" * padding

        strings = (
            encode_str(self.username)
            + encode_str(self.password)
            + encode_str(self.general_text_data)
            + encode_str(self.trade_account)
            + encode_str(self.hardware_identifier)
            + encode_str(self.client_name)
        )

        body = fixed + strings
        header = MessageHeader(size=MessageHeader.SIZE + len(body), type=self.TYPE)
        return header.pack() + body


@dataclass
class LogonResponse:
    """Server logon response."""

    protocol_version: int = 0
    result: int = 0
    result_text: str = ""
    reconnect_address: str = ""
    integer_1: int = 0
    server_name: str = ""
    market_depth_updates_best_bid_and_ask: int = 0
    trading_is_supported: int = 0
    oco_orders_supported: int = 0
    order_cancel_replace_supported: int = 0
    symbol_exchange_delimiter: str = ""
    security_definitions_supported: int = 0
    historical_price_data_supported: int = 0
    resubscribe_when_market_data_feed_available: int = 0
    market_depth_is_supported: int = 0
    one_historical_price_data_request_per_connection: int = 0
    bracket_orders_supported: int = 0
    use_integer_price_order_messages: int = 0
    uses_multiple_positions_per_symbol_and_trade_account: int = 0
    market_data_supported: int = 0

    TYPE = DTCMessageType.LOGON_RESPONSE

    @classmethod
    def unpack(cls, data: bytes) -> "LogonResponse":
        """Parse logon response."""
        body = data[MessageHeader.SIZE :]
        if len(body) < 8:
            return cls()

        version, result = struct.unpack("<ii", body[:8])

        # Parse null-terminated strings from remaining data
        strings_data = body[8:]
        strings = []
        current = b""
        for byte in strings_data:
            if byte == 0:
                strings.append(current.decode("utf-8", errors="replace"))
                current = b""
            else:
                current += bytes([byte])

        return cls(
            protocol_version=version,
            result=result,
            result_text=strings[0] if len(strings) > 0 else "",
            server_name=strings[2] if len(strings) > 2 else "",
        )


@dataclass
class Heartbeat:
    """Heartbeat message (sent both ways)."""

    num_dropped_messages: int = 0
    current_date_time: int = 0

    TYPE = DTCMessageType.HEARTBEAT

    def pack(self) -> bytes:
        body = struct.pack("<Iq", self.num_dropped_messages, self.current_date_time)
        header = MessageHeader(size=MessageHeader.SIZE + len(body), type=self.TYPE)
        return header.pack() + body

    @classmethod
    def unpack(cls, data: bytes) -> "Heartbeat":
        body = data[MessageHeader.SIZE :]
        if len(body) >= 12:
            dropped, dt = struct.unpack("<Iq", body[:12])
            return cls(num_dropped_messages=dropped, current_date_time=dt)
        return cls()


@dataclass
class MarketDataRequest:
    """Request market data subscription for a symbol."""

    request_action: int = RequestActionEnum.SUBSCRIBE
    symbol_id: int = 0  # Server assigns this
    symbol: str = ""
    exchange: str = ""

    TYPE = DTCMessageType.MARKET_DATA_REQUEST

    def pack(self) -> bytes:
        """Pack market data request."""
        # Fixed: action(2) + symbol_id(2) = 4 bytes
        fixed = struct.pack("<HH", self.request_action, self.symbol_id)

        # Variable: symbol and exchange as null-terminated strings
        symbol_bytes = self.symbol.encode("utf-8") + b"\x00"
        exchange_bytes = self.exchange.encode("utf-8") + b"\x00"

        body = fixed + symbol_bytes + exchange_bytes
        header = MessageHeader(size=MessageHeader.SIZE + len(body), type=self.TYPE)
        return header.pack() + body


@dataclass
class MarketDataReject:
    """Server rejection of market data request."""

    symbol_id: int = 0
    reject_text: str = ""

    TYPE = DTCMessageType.MARKET_DATA_REJECT

    @classmethod
    def unpack(cls, data: bytes) -> "MarketDataReject":
        body = data[MessageHeader.SIZE :]
        if len(body) >= 2:
            symbol_id = struct.unpack("<H", body[:2])[0]
            reject_text = body[2:].rstrip(b"\x00").decode("utf-8", errors="replace")
            return cls(symbol_id=symbol_id, reject_text=reject_text)
        return cls()


@dataclass
class MarketDataSnapshot:
    """Initial snapshot of market data for a symbol."""

    symbol_id: int = 0
    session_settlement_price: float = 0.0
    session_open_price: float = 0.0
    session_high_price: float = 0.0
    session_low_price: float = 0.0
    session_volume: float = 0.0
    session_num_trades: int = 0
    open_interest: int = 0
    bid_price: float = 0.0
    ask_price: float = 0.0
    ask_quantity: float = 0.0
    bid_quantity: float = 0.0
    last_trade_price: float = 0.0
    last_trade_volume: float = 0.0
    last_trade_date_time: float = 0.0
    bid_ask_date_time: float = 0.0
    session_settlement_date_time: int = 0
    trading_session_date: int = 0

    TYPE = DTCMessageType.MARKET_DATA_SNAPSHOT

    @classmethod
    def unpack(cls, data: bytes) -> "MarketDataSnapshot":
        body = data[MessageHeader.SIZE :]
        if len(body) < 100:
            return cls()

        # Parse the snapshot - Sierra uses doubles (8 bytes each)
        try:
            (
                symbol_id,
                settlement,
                session_open,
                session_high,
                session_low,
                session_volume,
                session_num_trades,
                open_interest,
                bid_price,
                ask_price,
                ask_qty,
                bid_qty,
                last_price,
                last_volume,
                last_dt,
                bid_ask_dt,
                settle_dt,
                trading_date,
            ) = struct.unpack("<H dddddd II dddd dd dd II", body[:142])

            return cls(
                symbol_id=symbol_id,
                session_settlement_price=settlement,
                session_open_price=session_open,
                session_high_price=session_high,
                session_low_price=session_low,
                session_volume=session_volume,
                session_num_trades=session_num_trades,
                open_interest=open_interest,
                bid_price=bid_price,
                ask_price=ask_price,
                ask_quantity=ask_qty,
                bid_quantity=bid_qty,
                last_trade_price=last_price,
                last_trade_volume=last_volume,
                last_trade_date_time=last_dt,
                bid_ask_date_time=bid_ask_dt,
            )
        except struct.error:
            return cls()


@dataclass
class MarketDataUpdateTrade:
    """Real-time trade update."""

    symbol_id: int = 0
    at_bid_or_ask: int = 0  # 0=unknown, 1=at bid, 2=at ask
    price: float = 0.0
    volume: float = 0.0
    date_time: float = 0.0  # Unix timestamp as double

    TYPE = DTCMessageType.MARKET_DATA_UPDATE_TRADE

    @classmethod
    def unpack(cls, data: bytes) -> "MarketDataUpdateTrade":
        body = data[MessageHeader.SIZE :]
        if len(body) < 26:
            return cls()

        try:
            # H=symbol_id(2), B=at_bid_or_ask(1), padding(1), d=price(8), d=volume(8), d=datetime(8)
            symbol_id, at_bid_or_ask = struct.unpack("<HB", body[:3])
            price, volume, dt = struct.unpack("<ddd", body[4:28])
            return cls(
                symbol_id=symbol_id,
                at_bid_or_ask=at_bid_or_ask,
                price=price,
                volume=volume,
                date_time=dt,
            )
        except struct.error:
            return cls()


@dataclass
class MarketDataUpdateTradeCompact:
    """Compact trade update (uses floats instead of doubles)."""

    symbol_id: int = 0
    at_bid_or_ask: int = 0
    price: float = 0.0
    volume: float = 0.0
    date_time: float = 0.0

    TYPE = DTCMessageType.MARKET_DATA_UPDATE_TRADE_COMPACT

    @classmethod
    def unpack(cls, data: bytes) -> "MarketDataUpdateTradeCompact":
        body = data[MessageHeader.SIZE :]
        if len(body) < 14:
            return cls()

        try:
            # H=symbol_id(2), B=at_bid_or_ask(1), padding(1), f=price(4), f=volume(4), I=datetime(4)
            symbol_id, at_bid_or_ask = struct.unpack("<HB", body[:3])
            price, volume, dt = struct.unpack("<ffI", body[4:16])
            return cls(
                symbol_id=symbol_id,
                at_bid_or_ask=at_bid_or_ask,
                price=float(price),
                volume=float(volume),
                date_time=float(dt),
            )
        except struct.error:
            return cls()


@dataclass
class MarketDataUpdateBidAsk:
    """Bid/ask update."""

    symbol_id: int = 0
    bid_price: float = 0.0
    bid_quantity: float = 0.0
    ask_price: float = 0.0
    ask_quantity: float = 0.0
    date_time: float = 0.0

    TYPE = DTCMessageType.MARKET_DATA_UPDATE_BID_ASK

    @classmethod
    def unpack(cls, data: bytes) -> "MarketDataUpdateBidAsk":
        body = data[MessageHeader.SIZE :]
        if len(body) < 34:
            return cls()

        try:
            symbol_id = struct.unpack("<H", body[:2])[0]
            bid_price, bid_qty, ask_price, ask_qty, dt = struct.unpack(
                "<ddddd", body[2:42]
            )
            return cls(
                symbol_id=symbol_id,
                bid_price=bid_price,
                bid_quantity=bid_qty,
                ask_price=ask_price,
                ask_quantity=ask_qty,
                date_time=dt,
            )
        except struct.error:
            return cls()


@dataclass
class MarketDataUpdateBidAskCompact:
    """Compact bid/ask update."""

    symbol_id: int = 0
    bid_price: float = 0.0
    bid_quantity: float = 0.0
    ask_price: float = 0.0
    ask_quantity: float = 0.0
    date_time: int = 0

    TYPE = DTCMessageType.MARKET_DATA_UPDATE_BID_ASK_COMPACT

    @classmethod
    def unpack(cls, data: bytes) -> "MarketDataUpdateBidAskCompact":
        body = data[MessageHeader.SIZE :]
        if len(body) < 18:
            return cls()

        try:
            symbol_id = struct.unpack("<H", body[:2])[0]
            bid_price, bid_qty, ask_price, ask_qty, dt = struct.unpack(
                "<ffffI", body[2:22]
            )
            return cls(
                symbol_id=symbol_id,
                bid_price=float(bid_price),
                bid_quantity=float(bid_qty),
                ask_price=float(ask_price),
                ask_quantity=float(ask_qty),
                date_time=dt,
            )
        except struct.error:
            return cls()


@dataclass
class MarketDataUpdateSessionHigh:
    """Session high price update."""

    symbol_id: int = 0
    price: float = 0.0
    trading_session_date: int = 0

    TYPE = DTCMessageType.MARKET_DATA_UPDATE_SESSION_HIGH

    @classmethod
    def unpack(cls, data: bytes) -> "MarketDataUpdateSessionHigh":
        body = data[MessageHeader.SIZE :]
        if len(body) >= 14:
            symbol_id, price, date = struct.unpack("<HdI", body[:14])
            return cls(symbol_id=symbol_id, price=price, trading_session_date=date)
        return cls()


@dataclass
class MarketDataUpdateSessionLow:
    """Session low price update."""

    symbol_id: int = 0
    price: float = 0.0
    trading_session_date: int = 0

    TYPE = DTCMessageType.MARKET_DATA_UPDATE_SESSION_LOW

    @classmethod
    def unpack(cls, data: bytes) -> "MarketDataUpdateSessionLow":
        body = data[MessageHeader.SIZE :]
        if len(body) >= 14:
            symbol_id, price, date = struct.unpack("<HdI", body[:14])
            return cls(symbol_id=symbol_id, price=price, trading_session_date=date)
        return cls()


@dataclass
class MarketDataUpdateSessionVolume:
    """Session volume update."""

    symbol_id: int = 0
    volume: float = 0.0
    trading_session_date: int = 0

    TYPE = DTCMessageType.MARKET_DATA_UPDATE_SESSION_VOLUME

    @classmethod
    def unpack(cls, data: bytes) -> "MarketDataUpdateSessionVolume":
        body = data[MessageHeader.SIZE :]
        if len(body) >= 14:
            symbol_id, volume, date = struct.unpack("<HdI", body[:14])
            return cls(symbol_id=symbol_id, volume=volume, trading_session_date=date)
        return cls()


# =============================================================================
# Message Parser
# =============================================================================


def parse_message(data: bytes) -> tuple[Optional[Any], int]:
    """
    Parse a DTC message from binary data.

    Returns:
        Tuple of (parsed message or None, bytes consumed)
    """
    if len(data) < MessageHeader.SIZE:
        return None, 0

    header = MessageHeader.unpack(data)

    if len(data) < header.size:
        return None, 0  # Incomplete message

    msg_data = data[: header.size]
    msg_type = header.type

    parsers = {
        DTCMessageType.ENCODING_RESPONSE: EncodingResponse.unpack,
        DTCMessageType.LOGON_RESPONSE: LogonResponse.unpack,
        DTCMessageType.HEARTBEAT: Heartbeat.unpack,
        DTCMessageType.MARKET_DATA_REJECT: MarketDataReject.unpack,
        DTCMessageType.MARKET_DATA_SNAPSHOT: MarketDataSnapshot.unpack,
        DTCMessageType.MARKET_DATA_UPDATE_TRADE: MarketDataUpdateTrade.unpack,
        DTCMessageType.MARKET_DATA_UPDATE_TRADE_COMPACT: MarketDataUpdateTradeCompact.unpack,
        DTCMessageType.MARKET_DATA_UPDATE_BID_ASK: MarketDataUpdateBidAsk.unpack,
        DTCMessageType.MARKET_DATA_UPDATE_BID_ASK_COMPACT: MarketDataUpdateBidAskCompact.unpack,
        DTCMessageType.MARKET_DATA_UPDATE_SESSION_HIGH: MarketDataUpdateSessionHigh.unpack,
        DTCMessageType.MARKET_DATA_UPDATE_SESSION_LOW: MarketDataUpdateSessionLow.unpack,
        DTCMessageType.MARKET_DATA_UPDATE_SESSION_VOLUME: MarketDataUpdateSessionVolume.unpack,
    }

    parser = parsers.get(msg_type)
    if parser:
        return parser(msg_data), header.size

    # Return raw header info for unknown message types
    return {"type": msg_type, "size": header.size, "raw": msg_data}, header.size
