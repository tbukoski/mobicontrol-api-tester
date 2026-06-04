# MobiControl API Tester

A cross-platform desktop GUI (Rust + egui) for exercising the SOTI MobiControl
REST API. Acquires an OAuth2 token, browses the swagger document, prompts for
parameters, invokes the endpoint, and writes the full response to a file.

## Features

- Credentials form: Client ID, Client Secret, Username, Password, FQDN
- Save / load credentials as an AES-GCM-256 encrypted file (passphrase-protected,
  via the `simple-encrypt` crate)
- Fetch the swagger document from
  `https://{FQDN}/MobiControl/api/swagger/v2/swagger.json`, with automatic
  fallback to a local `sample_swagger.json` next to the executable
- Method selector (GET / POST / PUT / DELETE) and filterable path list
- Per-operation parameter editor (path / query / body), with combo boxes for
  enums and booleans
- Output file picker (native file dialog via `rfd`)
- Run button: gets a Bearer token, invokes the call, writes the response body to
  the chosen file
- All network and file I/O runs on a background thread; the UI stays responsive
- OS-aware defaults: credentials and output paths default to the user's home
  directory on both Windows and Linux

## Build

Requires a stable Rust toolchain.

```bash
cargo build --release
```

The binary is produced at `target/release/mobicontrol-api-tester` (or
`mobicontrol-api-tester.exe` on Windows).

TLS is provided by `rustls`, so no OpenSSL development headers are required.

## Layout for Distribution

Place `sample_swagger.json` in the same directory as the executable. The app
will use it whenever the server fetch fails or the FQDN field is empty.

```
mobicontrol-api-tester(.exe)
sample_swagger.json
```

## Usage

1. Enter `Client ID`, `Client Secret`, `Username`, `Password`, and `FQDN`.
2. Click **Fetch swagger**. If the FQDN is reachable, the swagger is pulled from
   the server; otherwise the sample file is loaded.
3. Pick an HTTP method, type into the filter box to narrow the list, and click
   a path.
4. Fill in any parameters the operation requires. Required parameters are
   marked with `*`. Hover a parameter label to see its description.
5. Set the output file path (default is `~/mobicontrol_api_output.json`), or
   click **Browse** to pick one.
6. Click **Run**. The status line reports progress; the response body is
   pretty-printed in the response preview and written verbatim to the file.

### Saving and loading credentials

- **Save credentials**: prompts for a passphrase, encrypts the credential set
  with AES-GCM-256, and writes it (default `~/mobicontrol_credentials.enc`).
- **Load credentials**: prompts for the passphrase and applies the decrypted
  credentials to the current session.

The passphrase is never stored. If you lose it, the file cannot be recovered.

## Security Notes

- Credentials live in memory only during the session; clear them by closing the
  app.
- The encrypted credentials file uses AES-GCM-256 with the passphrase as key
  material. Choose a strong passphrase.
- The output file contains the raw API response, which can include sensitive
  device, user, or organization data. Treat it accordingly.

## Project Structure

```
src/
  main.rs          eframe entry point
  app.rs           UI state, event handling, background task coordination
  credentials.rs   Credentials struct + encrypted save / load
  swagger.rs       Minimal Swagger 2.0 model + parser
  auth.rs          OAuth2 token endpoint client
  api.rs           Swagger fetch + generic request invocation
  paths.rs         OS-agnostic default paths
```
