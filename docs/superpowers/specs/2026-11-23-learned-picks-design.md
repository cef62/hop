# Learned picks: the result you choose for a query comes first

Status: approved by the owner (2026-11-23).

## Intent

On another laptop the owner types "me", and hop lists Messages first and the Chrome app Meet second; the owner always picks Meet. Usage (frecency) only reorders results inside one match tier, so a word-start match ("Google Meet") never beats a prefix match ("Messages"), however often it is picked. hop should learn which result the owner picks for what they typed, like Raycast and Alfred.

## Approaches

1. **Learn picks per query (picked by the owner).** Remember query → chosen item; a learned item comes first for that query. Fixes the case exactly, leaves every other search alone.
2. Let overall usage outweigh match tiers. Simpler, but a favourite app would push out exact matches in unrelated searches. Not picked.

## Design

- **Recording.** When `run` opens an item (not `reveal`, not a calculator copy) with a non-empty query, record a pick: key = the folded, trimmed query (`fold` from `src/fuzzy.ts`, mirrored in Rust or sent folded by the frontend), at most 30 characters; value = the item id with a score (+1, the same weekly half-life decay as `history.rs`). A command records a pick only when it runs (not when it arms).
- **Store.** A separate `picks.json` in the app data dir: `{ "<key>": { "<id>": { "score", "at" } } }`, decayed and pruned like `history.json` (drop below `FORGET`), at most 500 keys (drop the stalest). Missing or corrupt → empty. `history.json` is unchanged, so usage counts and a downgrade keep working.
- **Serving.** A `picks` command (in `MAIN_COMMANDS` only) returns `{ key: { id: decayedScore } }`, loaded with frecency on `palette-shown`.
- **Ranking** (`search` in `src/rank.ts`): for query `q`, an item's pick score is the sum over keys `k` that are a prefix of `q` (including `q` itself) of its score under `k`. Items with a pick score > 0 that still match `q` (tier > 0) come first, ordered by pick score; the rest follow in today's order. So "me" → Meet also applies to "mee" and "meet", not to "mes" if Meet does not match it (it does not) — and a learned "mee" does not apply to "me".
- **Unaffected:** the empty query (RECENT), calculator leads, the two-Enter confirm (a learned Restart comes first but still needs two Enters), the window guard.
- **Not included:** a setting to turn it off, forgetting a single pick.

## Review focus

- A pick never surfaces an item that does not match the query.
- Typing past a learned key keeps the pick; deleting back below it drops it.
- Lock Screen / Sleep learned for a short query: first row, one Enter runs it — accepted (the owner typed and chose it before), but never on the empty query.
- Store growth and decay; corrupt file; concurrent writes with `history.json`.
- Performance: `search` still within the existing perf test budget with 500 keys.
