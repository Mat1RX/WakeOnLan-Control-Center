# Secure Wake-On-LAN API Server & Web UI

A high-performance, secure Rust API server (powered by `axum` and `tokio`) for waking up devices within your local network (Wake-On-LAN). Originally a simple Telegram bot, this project has been completely rewritten into a full-fledged **RESTful API Server** and a **Single Page Application (SPA)** frontend, supporting multi-user authentication and strict HTTPS enforcement.

## Features
- **Low Memory Footprint:** Designed to run on embedded home routers (like OpenWrt), utilizing Tokio's single-threaded `current_thread` runtime.
- **Strict HTTPS Security:** The server binds exclusively to encrypted TLS connections through `axum-server`. Plain HTTP requests are rejected at the socket layer.
- **Attack Prevention:** Mitigates timing-based authentication attacks via constant-time password comparisons (using the `subtle` crate). Includes brute-force protection and strict validation of IP addresses.
- **Static Frontend:** A beautiful, responsive "Glassmorphism" UI that can be seamlessly hosted on GitHub Pages, Vercel, or simply opened locally in any browser.

---

## Setup & Deployment (Backend)

1. Install the latest Rust toolchain.
2. Build the project: `cargo build --release`
3. Copy the configuration template: `cp config.toml.example config.toml`
4. Edit the `config.toml` file to include:
   - Your TLS certificates (`tls_cert_path` and `tls_key_path`). For local testing, you can generate self-signed ones via OpenSSL.
   - Authorized users under `[users]` and a highly random `jwt_secret`.
   - Your specific devices (names, MAC addresses, IPs, and timeouts) under the `[devices]` block.
5. Launch the server by passing the config path:
   `cargo run --release -- config.toml`

---

## API Documentation

The server exposes a REST API. All endpoints (except `/auth/login`) are protected and require a valid JWT token passed in the `Authorization: Bearer <token>` header.

### 1. Authentication
**`POST /auth/login`**
Exchanges valid user credentials for a temporary JWT token (valid for 24 hours).

**Request Body (JSON):**
```json
{
  "username": "admin",
  "password": "supersecretpassword"
}
```

**Response (200 OK):**
```json
{
  "token": "eyJ0eXAiOiJKV..."
}
```

*Note: Invalid credentials yield a `401 Unauthorized` response with an artificial 1-second delay to mitigate brute-force attacks.*

### 2. Get Device List
**`GET /api/devices`**
Returns an array of all device names configured in `config.toml`.

**Headers:** `Authorization: Bearer <token>`

**Response (200 OK):**
```json
{
  "devices": [
    "gaming_pc",
    "nas",
    "home_server"
  ]
}
```

### 3. Bulk Status Check (Ping)
**`GET /api/status`**
The server asynchronously pings all known devices via ICMP and returns their network statuses.

**Headers:** `Authorization: Bearer <token>`

**Response (200 OK):**
```json
{
  "devices": {
    "gaming_pc": "online",
    "nas": "offline",
    "home_server": "online"
  }
}
```

### 4. Single Device Status Check
**`GET /api/status/:name`**
Executes an immediate ICMP ping against the specific mapped IP of `<name>`.

**Headers:** `Authorization: Bearer <token>`

**Response (200 OK):**
```json
{
  "status": "online"
}
```
*Returns `404 Not Found` if the device name does not exist.*

### 5. Send Magic Packet (Wake-On-LAN)
**`POST /api/wake/:name`**
Broadcasts a WOL Magic Packet using the target's MAC address to the local network. After dispatching the packet, the **API blocks for the device's configured timeout duration** (waiting for the OS to boot up), then performs a final ping request to verify if the wake attempt was successful.

**Headers:** `Authorization: Bearer <token>`

**Response (200 OK):**
```json
{
  "status": "online"
}
```
*(The status reflects the final network state of the machine after the timeout period).*

---

## Frontend (Web UI)
The `frontend/` directory contains three static files (`index.html`, `app.js`, `styles.css`) powering the control dashboard, handling localStorage JWT management, CORS network fetching, and real-time status rendering.

Because this application relies entirely on client-side rendering without any tied backend templates:
1. You can push the `frontend/` folder directly to **GitHub Pages** (by initializing a repository and deploying static content).
2. When logging in via the frontend UI, you must specify the exact, reachable IP/Domain of your router API (e.g., `https://router.local:8443`).
*Note: Browsers inherently block cross-origin AJAX requests (Fetch API) if the target server uses an unverified (self-signed) SSL certificate. Before using the UI, ensure you have trusted the self-signed certificate on your device or utilize a valid SSL signature from a recognized Certificate Authority like Let's Encrypt.*
