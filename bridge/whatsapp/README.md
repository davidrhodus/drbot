# drbot WhatsApp Bridge

Node.js bridge for WhatsApp integration using the Baileys library.

## Setup

```bash
cd bridge/whatsapp
npm install
```

## Running

```bash
npm start
# or
node bridge.js
```

The bridge will start a WebSocket server on port 3001 (configurable via `PORT` env var).

## Environment Variables

- `PORT` - WebSocket server port (default: 3001)
- `LOG_LEVEL` - Logging level: trace, debug, info, warn, error (default: info)

## Protocol

All messages are JSON with a `type` field.

### Commands (Rust -> Bridge)

#### `init`
Initialize WhatsApp connection.
```json
{
  "type": "init",
  "session_dir": ".whatsapp"
}
```

#### `send_message`
Send a text message.
```json
{
  "type": "send_message",
  "id": "unique-id",
  "to": "1234567890@s.whatsapp.net",
  "text": "Hello!"
}
```

#### `send_media`
Send media (image, video, audio, document).
```json
{
  "type": "send_media",
  "id": "unique-id",
  "to": "1234567890@s.whatsapp.net",
  "media_type": "image",
  "data": "base64-encoded-data",
  "caption": "Optional caption",
  "filename": "document.pdf"
}
```

#### `get_qr`
Request QR code (triggers if not authenticated).
```json
{
  "type": "get_qr"
}
```

#### `status`
Check connection status.
```json
{
  "type": "status"
}
```

#### `disconnect`
Disconnect and logout.
```json
{
  "type": "disconnect"
}
```

### Events (Bridge -> Rust)

#### `connection`
Connection status changed.
```json
{
  "type": "connection",
  "status": "connecting" | "open" | "close" | "loggedOut"
}
```

#### `qr`
QR code for authentication.
```json
{
  "type": "qr",
  "qr": "qr-code-string"
}
```

#### `ready`
WhatsApp is ready to send/receive messages.
```json
{
  "type": "ready"
}
```

#### `message`
Incoming message.
```json
{
  "type": "message",
  "id": "message-id",
  "chat": "1234567890@s.whatsapp.net",
  "sender": "1234567890@s.whatsapp.net",
  "sender_name": "John Doe",
  "timestamp": 1234567890,
  "text": "Hello!",
  "from_me": false,
  "media_type": null,
  "media_url": null,
  "quoted_id": null
}
```

#### `sent`
Message sent confirmation.
```json
{
  "type": "sent",
  "id": "request-id",
  "message_id": "whatsapp-message-id"
}
```

#### `error`
Error occurred.
```json
{
  "type": "error",
  "error": "Error message",
  "id": "request-id"
}
```

## Usage with drbot

1. Start the bridge:
   ```bash
   cd bridge/whatsapp && npm start
   ```

2. Configure drbot:
   ```toml
   [channels.whatsapp]
   session_path = ".whatsapp"
   ```

3. The first time you connect, scan the QR code displayed in the terminal.

## Session Persistence

Sessions are stored in the `session_dir` directory (default: `.whatsapp`).
To logout and re-authenticate, delete this directory.
