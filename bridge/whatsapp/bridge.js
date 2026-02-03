/**
 * drbot WhatsApp Bridge
 *
 * WebSocket server that bridges drbot to WhatsApp via Baileys.
 *
 * Protocol:
 * - All messages are JSON with a "type" field
 * - Rust -> Bridge: init, send_message, send_media, get_qr, status, disconnect
 * - Bridge -> Rust: connection, qr, message, sent, error, ready
 */

const { default: makeWASocket, useMultiFileAuthState, DisconnectReason, fetchLatestBaileysVersion } = require('@whiskeysockets/baileys');
const { WebSocketServer } = require('ws');
const pino = require('pino');
const qrcode = require('qrcode-terminal');
const path = require('path');
const fs = require('fs');

const PORT = process.env.PORT || 3001;
const logger = pino({ level: process.env.LOG_LEVEL || 'info' });

let sock = null;
let wsClient = null;
let sessionDir = '.whatsapp';

// Send message to Rust client
function send(msg) {
    if (wsClient && wsClient.readyState === 1) {
        wsClient.send(JSON.stringify(msg));
    }
}

// Initialize WhatsApp connection
async function initWhatsApp() {
    logger.info({ sessionDir }, 'Initializing WhatsApp connection');

    // Ensure session directory exists
    if (!fs.existsSync(sessionDir)) {
        fs.mkdirSync(sessionDir, { recursive: true });
    }

    const { state, saveCreds } = await useMultiFileAuthState(sessionDir);
    const { version } = await fetchLatestBaileysVersion();

    sock = makeWASocket({
        version,
        logger: pino({ level: 'silent' }),
        auth: state,
        printQRInTerminal: false,
        browser: ['drbot', 'Chrome', '120.0.0'],
    });

    // Handle connection updates
    sock.ev.on('connection.update', async (update) => {
        const { connection, lastDisconnect, qr } = update;

        if (qr) {
            logger.info('QR code received');
            // Print to terminal for convenience
            qrcode.generate(qr, { small: true });
            // Send to Rust client
            send({ type: 'qr', qr });
        }

        if (connection === 'close') {
            const statusCode = lastDisconnect?.error?.output?.statusCode;
            const shouldReconnect = statusCode !== DisconnectReason.loggedOut;

            logger.info({ statusCode, shouldReconnect }, 'Connection closed');
            send({ type: 'connection', status: 'close' });

            if (shouldReconnect) {
                logger.info('Reconnecting...');
                setTimeout(initWhatsApp, 3000);
            } else {
                send({ type: 'connection', status: 'loggedOut' });
            }
        } else if (connection === 'open') {
            logger.info('Connected to WhatsApp');
            send({ type: 'connection', status: 'open' });
            send({ type: 'ready' });
        } else if (connection === 'connecting') {
            send({ type: 'connection', status: 'connecting' });
        }
    });

    // Save credentials when they update
    sock.ev.on('creds.update', saveCreds);

    // Handle incoming messages
    sock.ev.on('messages.upsert', async ({ messages, type }) => {
        if (type !== 'notify') return;

        for (const msg of messages) {
            // Skip status updates
            if (msg.key.remoteJid === 'status@broadcast') continue;

            const messageContent = msg.message;
            if (!messageContent) continue;

            // Extract text content
            let text = null;
            if (messageContent.conversation) {
                text = messageContent.conversation;
            } else if (messageContent.extendedTextMessage) {
                text = messageContent.extendedTextMessage.text;
            } else if (messageContent.imageMessage?.caption) {
                text = messageContent.imageMessage.caption;
            } else if (messageContent.videoMessage?.caption) {
                text = messageContent.videoMessage.caption;
            }

            // Determine media type
            let mediaType = null;
            let mediaUrl = null;
            if (messageContent.imageMessage) {
                mediaType = 'image';
            } else if (messageContent.videoMessage) {
                mediaType = 'video';
            } else if (messageContent.audioMessage) {
                mediaType = 'audio';
            } else if (messageContent.documentMessage) {
                mediaType = 'document';
            }

            // Get sender info
            const isGroup = msg.key.remoteJid.endsWith('@g.us');
            const sender = isGroup
                ? msg.key.participant || msg.key.remoteJid
                : msg.key.remoteJid;

            // Get push name (display name)
            const senderName = msg.pushName || null;

            // Get quoted message ID if this is a reply
            const quotedId = messageContent.extendedTextMessage?.contextInfo?.stanzaId || null;

            const outMsg = {
                type: 'message',
                id: msg.key.id,
                chat: msg.key.remoteJid,
                sender,
                sender_name: senderName,
                timestamp: msg.messageTimestamp,
                text,
                from_me: msg.key.fromMe || false,
                media_type: mediaType,
                media_url: mediaUrl,
                quoted_id: quotedId,
            };

            logger.debug({ msg: outMsg }, 'Received message');
            send(outMsg);
        }
    });

    return sock;
}

// Handle messages from Rust client
async function handleMessage(data) {
    let msg;
    try {
        msg = JSON.parse(data);
    } catch (e) {
        logger.error({ error: e.message }, 'Invalid JSON');
        send({ type: 'error', error: 'Invalid JSON' });
        return;
    }

    logger.debug({ msg }, 'Received command');

    switch (msg.type) {
        case 'init':
            sessionDir = msg.session_dir || '.whatsapp';
            await initWhatsApp();
            break;

        case 'send_message':
            if (!sock) {
                send({ type: 'error', error: 'Not connected', id: msg.id });
                return;
            }
            try {
                const result = await sock.sendMessage(msg.to, { text: msg.text });
                send({ type: 'sent', id: msg.id, message_id: result.key.id });
            } catch (e) {
                logger.error({ error: e.message }, 'Failed to send message');
                send({ type: 'error', error: e.message, id: msg.id });
            }
            break;

        case 'send_media':
            if (!sock) {
                send({ type: 'error', error: 'Not connected', id: msg.id });
                return;
            }
            try {
                let content;
                const mediaBuffer = Buffer.from(msg.data, 'base64');

                switch (msg.media_type) {
                    case 'image':
                        content = { image: mediaBuffer, caption: msg.caption };
                        break;
                    case 'video':
                        content = { video: mediaBuffer, caption: msg.caption };
                        break;
                    case 'audio':
                        content = { audio: mediaBuffer, mimetype: 'audio/mp4' };
                        break;
                    case 'document':
                        content = {
                            document: mediaBuffer,
                            fileName: msg.filename || 'document',
                            caption: msg.caption
                        };
                        break;
                    default:
                        throw new Error(`Unknown media type: ${msg.media_type}`);
                }

                const result = await sock.sendMessage(msg.to, content);
                send({ type: 'sent', id: msg.id, message_id: result.key.id });
            } catch (e) {
                logger.error({ error: e.message }, 'Failed to send media');
                send({ type: 'error', error: e.message, id: msg.id });
            }
            break;

        case 'get_qr':
            // If we have a socket, request will trigger QR
            // Otherwise, need to initialize first
            if (!sock) {
                send({ type: 'error', error: 'Not initialized. Send init first.' });
            }
            break;

        case 'status':
            send({
                type: 'connection',
                status: sock ? 'open' : 'close'
            });
            break;

        case 'disconnect':
            if (sock) {
                await sock.logout();
                sock = null;
            }
            send({ type: 'connection', status: 'close' });
            break;

        default:
            send({ type: 'error', error: `Unknown command: ${msg.type}` });
    }
}

// Start WebSocket server
const wss = new WebSocketServer({ port: PORT });

wss.on('listening', () => {
    logger.info({ port: PORT }, 'WhatsApp bridge WebSocket server started');
    console.log(`WhatsApp bridge listening on ws://localhost:${PORT}`);
});

wss.on('connection', (ws) => {
    logger.info('Rust client connected');

    // Only allow one client
    if (wsClient) {
        wsClient.close();
    }
    wsClient = ws;

    ws.on('message', handleMessage);

    ws.on('close', () => {
        logger.info('Rust client disconnected');
        wsClient = null;
    });

    ws.on('error', (e) => {
        logger.error({ error: e.message }, 'WebSocket error');
    });
});

// Handle process termination
process.on('SIGINT', async () => {
    logger.info('Shutting down...');
    if (sock) {
        await sock.end();
    }
    process.exit(0);
});

process.on('SIGTERM', async () => {
    logger.info('Shutting down...');
    if (sock) {
        await sock.end();
    }
    process.exit(0);
});
