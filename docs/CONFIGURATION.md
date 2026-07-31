# Configuration

Vision-Core v1.0.4 reads runtime settings from environment variables when
`Settings::from_env()` constructs the default settings object. There are no
command-line configuration flags.

Invalid values for several scalar settings still fall back silently.
`TOKIO_WORKER_THREADS` and `VISION_DATA_DIR` now have explicit startup
validation as documented below.

## Variables

| Variable | Type and example | Default | Invalid-value behavior | Consumed |
| --- | --- | --- | --- | --- |
| `VISION_DATA_DIR` | Path, e.g. `C:\vision-data` | `./data` | Empty, whitespace-only, padded, inaccessible, unwritable, file-valued, or unusable database locations fail before storage opens | Yes |
| `VISION_HTTP_PORT` | `u16`, e.g. `7070` | `7070` | Non-numeric or out-of-range values silently use the default | Yes |
| `VISION_P2P_PORT` | `u16`, e.g. `7072` | `7072` | Non-numeric or out-of-range values silently use the default | Yes |
| `VISION_P2P_ADVERTISED_HOST` | Host/IP, e.g. `node.example.org` | unset | Whitespace is trimmed; an empty value becomes unset; syntax is validated later where applicable | Yes |
| `VISION_P2P_ADVERTISED_PORT` | Nonzero `u16`, e.g. `7072` | unset | Invalid, out-of-range, and zero values become unset | Yes |
| `VISION_ALLOW_PRIVATE_PEERS` | `1`, `true`, or another string | `true` | Only case-insensitive `true` or `1` means true; every present other value means false | Yes |
| `VISION_MINER_ADDRESS` | 64 lowercase hexadecimal characters | 64 zeroes | Invalid input silently becomes the zero address | Yes |
| `VISION_MINING` | `1`, `true`, or another string | `false` | Only case-insensitive `true` or `1` means true; every present other value means false | Yes |
| `VISION_MINING_THREADS` | Unsigned integer, e.g. `4` | `0` | Parse failure silently becomes zero | Parsed but currently unused |
| `VISION_ALPHA_AIRDROP_ENABLED` | `1`, `true`, or another string | `false` | Only case-insensitive `true` or `1` means true; every present other value means false | Yes |
| `VISION_SEED_PEERS` | Comma, semicolon, or newline-delimited addresses | Compiled `DEFAULT_SEED_PEERS` | Entries are trimmed; empty entries are removed; an explicitly empty string produces an empty list | Yes |

The implemented data-directory policy is defined in
[VISION_DATA_DIR_POLICY.md](VISION_DATA_DIR_POLICY.md). Missing input retains
`./data`. Relative paths remain accepted and are resolved against the process
working directory. The effective location is reported before `chain.db` opens.
An explicitly invalid value never falls back to another directory.

`RUST_LOG` is consumed by `tracing-subscriber`, not by `Settings`. It accepts a
standard tracing filter such as `vision_core=debug`; when missing or invalid,
the node uses `info`.

`TOKIO_WORKER_THREADS` is consumed before `Settings` while the asynchronous
runtime is constructed. When missing, it uses the number of available logical
CPUs, with the existing platform fallback of four. A present value must be a
positive `usize` integer with no surrounding whitespace. Zero, malformed,
negative, whitespace-padded, and overflowing values stop startup before the
Tokio runtime is constructed. The error identifies the variable, rejected
value, and positive-integer requirement.

## Address construction

The HTTP and P2P listen settings are built as `0.0.0.0:<port>`. The current
settings model does not provide an environment variable for choosing a
different bind host.

The advertised P2P host and port are optional and are used to form durable peer
identity information. `VISION_ALLOW_PRIVATE_PEERS` controls whether loopback,
private, or link-local advertised addresses may be accepted. Its current
default is permissive (`true`).

## Mining

Mining is disabled by default. Enabling `VISION_MINING` creates the runtime
miner manager. `VISION_MINER_ADDRESS` selects the canonical reward recipient.
If it is absent or invalid, the current implementation uses 64 zeroes.

`VISION_MINING_THREADS` is parsed into `Settings::mining_threads`, where zero is
documented as logical CPU count, but v1.0.4 does not consume this field. Setting
it currently has no effect.

## Alpha funding endpoint

`VISION_ALPHA_AIRDROP_ENABLED=true` conditionally registers
`POST /alpha/airdrop`. This endpoint mutates local chain state and is explicitly
development-only. When disabled, the route is not registered.

## Configuration file status

Source documentation mentions extending `Settings::from_env()` to read a TOML
file if `VISION_CONFIG` is set. No such loading is implemented in v1.0.4, and
`VISION_CONFIG` has no effect.

## Planned strict-validation migration

Future configuration-hardening tranches may reject additional scalar values
that currently fall back silently. Each is an intentional runtime-behavior
change and must include migration notes.

Expected future work includes:

- reporting the exact invalid variable and accepted format;
- rejecting invalid ports and boolean values;
- rejecting invalid miner addresses instead of substituting zeroes;
- deciding whether to implement `VISION_CONFIG` or remove its source claim;
- making the private-peer default and operational consequences explicit;
- either consuming `VISION_MINING_THREADS` or removing the setting.

Operators relying on silent fallback should normalize their environment before
that migration.

## Data-directory migration guidance

Operators should leave `VISION_DATA_DIR` unset to retain `./data`, or set it to
a non-empty path without surrounding whitespace. Relative paths resolve from
the process working directory. The selected directory must already exist or
have an existing writable parent, and `<effective-data-directory>/chain.db`
must be absent or a directory.

Startup now fails before database initialization when the explicit value or
effective location is unusable. Correct the environment or filesystem
permissions and restart. Do not move, delete, replace, or recreate an existing
`chain.db` as part of configuration correction.
