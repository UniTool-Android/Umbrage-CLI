# Adbify runtime integration

## Runtime ownership

Adbify owns Android USB permission and the active `UsbDeviceConnection`.
The terminal child process must not independently open USBFS or attempt to
request Android USB permission.

Adbify starts a loopback TCP IPC server from `TerminalService` and passes
these private environment variables to the terminal shell:

- `ADBIFY_USB_SOCKET=127.0.0.1:<ephemeral-port>`
- `ADBIFY_USB_TOKEN=<32-byte-random-token-as-hex>`

The server binds only to `127.0.0.1`. Each client must present the per-service
random token before the server sends device metadata or accepts USB requests.
The token is not persisted and changes whenever the service restarts.

The terminal child process and the Android app run under the same application
UID. The IPC token provides an additional barrier against unrelated local
processes. Do not expose the server on a wildcard or external network interface.

## Permission and transport

The user selects an attached MediaTek device from Adbify's **Connect MTK USB**
menu action. Adbify requests USB permission through `UsbManager`, opens the
device, claims an interface containing bulk IN and OUT endpoints, and retains
the resulting connection for the IPC session.

On Android, the CLI's default `--backend auto` and explicit
`--backend adbify` use the IPC transport. The transport implements
Penumbra's `MtkPort` interface and forwards bulk and control transfers to
Adbify's retained `UsbDeviceConnection`.

Explicit `--backend libusb` remains available for environments where direct
USBFS access is deliberately configured. It is not the default Android path.

## Protocol v1

All integer fields use network byte order (big-endian). The client sends the
32-byte authentication token immediately after connecting. The server replies
with a 10-byte handshake:

| Offset | Size | Meaning |
|---:|---:|---|
| 0 | 4 | ASCII `UMBR` |
| 4 | 2 | USB vendor ID |
| 6 | 2 | USB product ID |
| 8 | 1 | connection type: 0 BROM, 1 Preloader, 2 DA |
| 9 | 1 | protocol version (1) |

Requests are serialized on one client connection:

- Opcode 1: bulk IN; request contains length and timeout; response contains
  status, actual length, and received bytes.
- Opcode 2: bulk OUT; request contains timeout, length, and payload; response
  contains status and transferred length.
- Opcode 3: control IN; request contains request type, request, value, index,
  timeout, and length; response contains status, actual length, and bytes.
- Opcode 4: control OUT; request contains the same control fields followed by
  payload; response contains status and a zero payload length.

Bulk transfers are capped at 16 KiB per IPC request for compatibility with
Android's `UsbDeviceConnection.bulkTransfer`. Control payloads are capped at
1 MiB. A nonzero status indicates an Android USB transfer failure.

## Lifecycle and limitations

The server is owned by `TerminalService`. A USB session must be selected in
Adbify before running a device command. The server accepts one CLI client at a
time so USB operations cannot interleave. Closing the CLI connection releases
the IPC client but does not close the user's USB session; the session remains
available until replaced or the service stops.

The first implementation supports one selected device and a single claimed
bulk IN/OUT interface. USB re-enumeration requires a new Android permission
and session and is not performed transparently by the IPC transport.

## Native deliverables

The workflow builds only `arm64-v8a` and publishes:

- `libumbrage_cli.so`
- `umbrage-cli`

The artifact does not include or replace Adbify-managed `libusb.so` or
`libusb_bridge.so`. The Android default path uses the app-owned USB session
instead of relying on dynamic linker configuration for USB permission.
