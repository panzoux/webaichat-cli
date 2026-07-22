# Protocol Specification

## WebSocket Message Format

All messages are JSON with a consistent structure:

```json
{
  "type": "event_name",
  "provider": "chatgpt",
  "timestamp": 1234567890,
  "payload": { ... }
}
```

## Event Types

### Connect (Browser → Bridge)

Browser registers with the bridge.

```json
{
  "type": "Connect",
  "provider": "chatgpt",
  "version": "0.1.0"
}
```

### Ready (Bridge → Browser)

Bridge confirms connection.

```json
{
  "type": "Ready",
  "version": "0.1.0"
}
```

### SendMessage (Runtime → Bridge → Browser)

Send a chat message to an AI provider.

```json
{
  "type": "SendMessage",
  "provider": "chatgpt",
  "message": "Hello, how are you?"
}
```

### MessageStart (Browser → Bridge → Runtime)

Generation has started.

```json
{
  "type": "MessageStart",
  "provider": "chatgpt",
  "message_id": "msg_1234567890"
}
```

### MessageChunk (Browser → Bridge → Runtime)

Streaming chunk of the response.

```json
{
  "type": "MessageChunk",
  "provider": "chatgpt",
  "message_id": "msg_1234567890",
  "index": 0,
  "content": "Hello"
}
```

### MessageEnd (Browser → Bridge → Runtime)

Generation is complete.

```json
{
  "type": "MessageEnd",
  "provider": "chatgpt",
  "message_id": "msg_1234567890"
}
```

### Cancel (Runtime → Bridge → Browser)

Cancel ongoing generation.

```json
{
  "type": "Cancel",
  "provider": "chatgpt",
  "message_id": "msg_1234567890"
}
```

### Cancelled (Browser → Bridge → Runtime)

Cancellation acknowledged.

```json
{
  "type": "Cancelled",
  "provider": "chatgpt",
  "message_id": "msg_1234567890"
}
```

### Error (Any → Any)

Error occurred.

```json
{
  "type": "Error",
  "provider": "chatgpt",
  "message": "Could not find input textarea"
}
```

### Ping (Any → Any)

Heartbeat ping.

```json
{
  "type": "Ping",
  "timestamp": 1234567890
}
```

### Pong (Any → Any)

Heartbeat pong.

```json
{
  "type": "Pong",
  "timestamp": 1234567890
}
```

## Connection Flow

1. Browser connects to Bridge WebSocket
2. Browser sends `Connect` event
3. Bridge responds with `Ready` event
4. Runtime sends `SendMessage` event
5. Bridge routes to Browser
6. Browser sends `MessageStart` event
7. Browser sends `MessageChunk` events
8. Browser sends `MessageEnd` event

## Reconnection

If connection is lost:
1. Browser detects disconnection
2. Browser waits `RECONNECT_DELAY` (3000ms)
3. Browser reconnects to Bridge
4. Browser sends `Connect` event again
5. Bridge handles reconnection gracefully

## Heartbeat

- Browser sends `Ping` every 30 seconds
- Bridge responds with `Pong`
- If no `Pong` received within 10 seconds, connection is considered dead
