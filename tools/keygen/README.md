# ccu-keygen — activation key generator (Linux only)

Standalone generator for Chess Coach Utility activation codes. It ships
**beside** the app (copy it to `releases/keygen/`); it is never embedded in
the app and must not be distributed to customers.

## The model

One code, one device, done:

1. You generate a 20-character code (`CCU5` + 16 base36 characters, shown
   as `CCU5A-BCDEF-GHIJK-LMNOP`) and register it with the activation
   service.
2. The customer enters it once in the app. The app sends the code, the
   workspace name and its device ID to the service.
3. The service marks a seat used and returns an Ed25519-signed licence.
   From then on the app verifies that licence **offline** against the
   public key compiled into `core/licenseConfig.js`.

The activation service holds the only private key that can sign a licence.
The app cannot mint or extend one: expiry, workspace and device ID are
inside the signature, and a code is single-use per seat. A lost or leaked
code only affects that one code — the service stores only SHA-256 hashes.

The retired schemes are gone: there is no shared `KEY_SECRET` in the app
and no `.ccukey` file. Anyone who extracts the app's files still cannot
mint a licence.

## First-time setup

On the machine that will own the licensing key:

```bash
ccu-keygen init                 # creates ~/.config/ccu-keygen/keys.json (0600)
ccu-keygen public-keys          # the activation-2026 public key
```

Put that public key into `LICENSE_PUBLIC_KEYS` in `core/licenseConfig.js`
and rebuild the app. Back up `keys.json`; without it you cannot sign
licences or renew customers.

Export the private key for the activation service:

```bash
ccu-keygen export-activation-key --out activation.key
```

Start `activation/server.mjs` with `CCU_ACTIVATION_PRIVATE_KEY` (the file's
contents), `CCU_ADMIN_TOKEN` (a long random string) and a persistent
`CCU_ACTIVATION_DATA` path. See `activation/README.md`.

Check the link end to end:

```bash
ccu-keygen status
```

It prints the service key and says whether it matches this machine's
keyring. A mismatch means every licence the service signs will be rejected
by the app — fix it before selling a key.

## Daily use

Graphical interface (also the default with no command):

```bash
ccu-keygen gui
```

Pick a school (or switch to Coaching), choose the duration, number of
codes and seats, then **Generate codes**. **Register with service** sends
the batch to the activation service in one step. Service URL and admin
token are set once under **Service settings** and stored in
`~/.config/ccu-keygen/` (token mode `0600`).

Command line:

```bash
# Generate 10 one-device codes for 12 months and register them now
ccu-keygen gen-codes --school "Port Shepstone High School" \
    --months 12 --count 10 --publish

# Same, for a private coaching business, saved as a batch file
ccu-keygen gen-codes --coach "Kingsway Chess Academy" \
    --years 1 --count 3 --out kingsway.json --publish
```

`--publish` needs the admin token. It is read from `--admin-token`,
`CCU_ADMIN_TOKEN`, or the token saved by the GUI. The service URL is read
from `--server`, `CCU_ACTIVATION_URL`, or the saved setting.

The expiry is fixed when the code is generated, not when it is activated,
so hand codes out promptly. `--out` writes the import batch (which contains
the plaintext codes); delete it after you have given the codes out. The
service stores only hashes.

Useful operations:

```bash
# Replace a customer's computer: free the old device's seat
ccu-keygen release --code CCU5A-BCDEF-GHIJK-LMNOP --device <Device ID from the app>

# Disable an unused code (already-delivered offline licences keep working)
ccu-keygen revoke --code CCU5A-BCDEF-GHIJK-LMNOP

# List the school registry, check the service/keyring, regenerate the key
ccu-keygen list
ccu-keygen status
ccu-keygen init --force        # DANGER: invalidates every existing licence
```

`release` and `revoke` require the admin token; the app never calls them
(the customer sends you the Device ID shown in Settings → Support).

The activation service can run on a VPS (`activation/server.mjs`) or, with no
domain, on Deno Deploy (`activation/deno/README.md`). Both speak the same API.

## Configuration files

All under `~/.config/ccu-keygen/` (or `CCU_KEYGEN_HOME`):

| File | Purpose |
| --- | --- |
| `keys.json` | Activation signing key pair. **Back this up.** |
| `admin-token` | Admin token used by the GUI to register/release/revoke (0600). |
| `server` | Activation service URL (defaults to `https://ccu-activation.ccu.deno.net`). |
| `db-path` | Last database folder chosen in the GUI. |

## Build and test

```bash
cargo build --release
cargo test
node scripts/e2e.mjs          # real keygen → Node server → signed licence
CCU_E2E_SERVER=deno node scripts/e2e.mjs   # same contract against Deno KV
```

`scripts/e2e.mjs` starts the activation service in a temp directory with a
throwaway key, generates codes with the Rust binary, imports them, activates
one on two devices, releases a seat and revokes a code. It does not touch
production keys or the production service. Set `CCU_E2E_SERVER=deno` to run
the same contract against `activation/deno/server.ts`.

Install into the release folder:

```bash
make install                  # releases/keygen/ccu-keygen
```
