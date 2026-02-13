# MCP (OpenClaw parity)

`drbot` supports OpenClaw's `mcp` tool for interacting with Model Context Protocol (MCP) servers.

## Configuration file

The gateway reads MCP server definitions from `mcp.json` inside the OpenClaw state directory.

- Default state dir: derived from your configured storage directory
- Override with env vars (checked in this order): `OPENCLAW_STATE_DIR`, `CLAWDBOT_STATE_DIR`, `OPENCLAW_HOME`

## Minimal `mcp.json` example (JSON5)

`drbot` parses this file as JSON5 (comments and trailing commas are allowed).

```json5
{
  servers: {
    my_stdio_server: {
      kind: "stdio",
      command: "python3",
      args: ["-m", "my_mcp_server"],
    },

    my_http_server: {
      kind: "http",
      url: "https://example.com/mcp",
    },
  },
}
```

## HTTP SSRF policy

For `kind: "http"` servers, the gateway enforces an SSRF policy.

- Default: block private/loopback targets (including `localhost` / `127.0.0.1`)
- To allow private targets: set `DRBOT_OPENCLAW_MCP_ALLOW_PRIVATE=1`
- To allow specific hosts without allowing all private targets: set `DRBOT_OPENCLAW_MCP_ALLOWED_HOSTNAMES=a.example.com,b.example.com`

## Tool usage (examples)

- List configured servers: `{ "action": "servers" }`
- List tools: `{ "action": "tools.list", "server": "my_stdio_server" }`
- Call a tool: `{ "action": "tools.call", "server": "my_stdio_server", "name": "tool_name", "arguments": { } }`
- List resources: `{ "action": "resources.list", "server": "my_stdio_server" }`
- Read a resource: `{ "action": "resources.read", "server": "my_stdio_server", "uri": "file:///..." }`
- List prompts: `{ "action": "prompts.list", "server": "my_stdio_server" }`
- Get a prompt: `{ "action": "prompts.get", "server": "my_stdio_server", "name": "prompt_name", "promptArgs": { } }`
