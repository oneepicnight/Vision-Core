# HTTP API

This document records the v1.0.4 Axum router. The API is under active
development and has no published compatibility guarantee.

The default listener is `0.0.0.0:7070`. Responses are JSON. There is no
authentication or TLS layer in the core router.

## Endpoints

| Method | Route | Request | Successful response | Status behavior |
| --- | --- | --- | --- | --- |
| `GET` | `/status` | None | Node status snapshot | `200` |
| `GET` | `/peers` | None | Array of peer entries | `200`; currently always an empty array |
| `GET` | `/balance/:address` | Address path string | `{address, exists, balance}` | `200`, including unknown/malformed keys |
| `GET` | `/nonce/:address` | Address path string | `{address, exists, nonce}` | `200`, including unknown/malformed keys |
| `GET` | `/transaction/:txid` | Transaction ID path string | Lookup snapshot with location and optional transaction | `200`, including not-found |
| `GET` | `/mining/info` | None | Mining/recovery snapshot | `200` |
| `POST` | `/transactions` | Canonical signed `Tx` JSON | Admission response | `200`, `400`, or `422` |
| `POST` | `/alpha/airdrop` | `{address, amount}` | Development funding response | Conditionally registered; `200`, `400`, `422`, or `500` |

## Status

`GET /status` returns the current snapshot defined by `NodeStatusSnapshot`,
including chain identity and height, tip and state-root information, mempool and
peer counts, mining status, and recovery status. Field additions or changes are
not yet governed by an API-versioning policy.

## Peers

The intended peer entry contains:

```json
{
  "addr": "127.0.0.1:7072",
  "state": "Connected",
  "height": 42,
  "outbound": true,
  "height_age_secs": 3
}
```

The handler is currently a stub and returns `[]`; it is not wired to
`PeerManager`.

## Balance and nonce

Examples:

```json
{"address":"<path value>","exists":true,"balance":42000}
```

```json
{"address":"<path value>","exists":true,"nonce":7}
```

Unknown or invalid-looking address strings are represented with `exists:
false` and a zero value rather than a `400` or `404`. Clients must inspect
`exists`.

## Transaction lookup

The response reports the requested ID, a location such as canonical or mempool,
and optional transaction/block information as defined by
`TransactionLookupSnapshot`. A missing transaction is represented in the JSON
snapshot rather than by HTTP `404`.

## Mining information

`GET /mining/info` returns:

```json
{
  "enabled": false,
  "height": 0,
  "difficulty": 1,
  "epoch": 0,
  "active": false,
  "recovery_state": "normal",
  "paused_reason": null,
  "hash_rate_estimate": null
}
```

Values are runtime snapshots and the example is illustrative, not a promised
state.

## Transaction submission

`POST /transactions` accepts a JSON object matching the serialized `Tx` fields:

```json
{
  "nonce": 0,
  "sender_pubkey": "<64 lowercase hex characters>",
  "module": "cash",
  "method": "transfer",
  "args": [123, 34, 116, 111, 34, 58, 34, 46, 46, 46, 34, 125],
  "tip": 1,
  "fee_limit": 201,
  "sig": "<128 lowercase hex characters>"
}
```

`args` is a byte array. For `cash::transfer`, those bytes are canonical JSON for
an object containing `to` and `amount`.

Accepted submissions return HTTP `200`:

```json
{
  "status": "accepted",
  "tx_id": "<canonical id>",
  "current_nonce": 0,
  "decision": {"kind": "accept"}
}
```

A replacement includes `decision.kind = "replace"` and `evict_tx_id`.
Malformed JSON returns HTTP `400`, status `malformed_request`, and an error
object. Valid JSON rejected by admission returns HTTP `422`, status `rejected`,
the canonical ID/current nonce where available, and an error object with a
stable-looking code. These codes are tested but have not yet been declared a
versioned public contract.

Current rejection codes include:

- `tx_too_large`
- `missing_sender_pubkey`
- `missing_signature`
- `unsupported_module_method`
- `bad_transfer_args`
- `invalid_transfer_destination`
- `transfer_amount_zero`
- `transfer_to_self`
- `fee_limit_too_low`
- sender/signature format and verification codes
- `duplicate_canonical_tx_id`
- `stale_nonce`
- `nonce_gap`
- `duplicate_sender_nonce`

## Alpha airdrop

The route exists only when `VISION_ALPHA_AIRDROP_ENABLED=true`.

Request:

```json
{"address":"<64 lowercase hex characters>","amount":1000}
```

Responses include `status`, the fixed scope `alpha_dev_only`, optional balance
and canonical state fields, and an optional `{code, message}` error. Disabled
behavior is internally modeled as `404`, but because the router omits the route
when disabled, ordinary disabled requests receive Axum's unmatched-route
response instead.

## Known inconsistencies

- Read-only not-found results use HTTP `200`; mutation errors use `400`/`422`.
- Error envelopes are separately implemented for transaction and alpha routes.
- `/peers` advertises a response type but is not connected to live state.
- The disabled alpha handler has a structured `404` branch, while normal router
  construction omits the route entirely.
- No OpenAPI document or explicit API version currently exists.

These are documented facts, not changes made by the developer-foundation
tranche.
