# Package saves during development

Ordinary Luau behavior edits can reopen the same save after stopping and starting
the server. This includes imported helper edits and extracting helper modules.
The server still owns inventories, entities, owner state and scheduled deadlines;
these values are loaded from the save rather than reconstructed from script locals.
Manual `reload packages` uses the same persistent contracts, with stricter live
startup equality checks described in [package development](PACKAGE-DEVELOPMENT.md).

## Compatibility boundaries

| Change | Existing save |
| --- | --- |
| Behavior source with unchanged declarations and dependency contracts | Compatible |
| Unrelated package addition or version change | Does not change another package's handler identity; its own content is validated normally |
| Client-only source in a generator package | Compatible if other declarations remain equal |
| Explicit schema/revision, binary layout, entry module, relevant package version, dependencies or capabilities | Contract changes can reject the save |
| Registered content definitions, catalog-bound textures/models | Existing contract fingerprints still apply |
| Scripted generator server/shared source, including transitive package dependencies | Rejects even at the same declared revision |
| Added/removed generator or changed generator revision | Rejects |

Saved handler identities include the declaring package's transitive dependency
metadata, without including ordinary source bytes. This remains conservative:
explicit package-version or capability changes can fence saved state even when a
human considers them behavior-only. Keep versions and declared contracts stable
while iterating compatible behavior. When the interpretation of persisted binary
state changes, declare the appropriate new schema/revision. The host cannot infer
that semantic change from source. Do not rename persistent namespaced keys or
reuse builtin numeric IDs for different content.

Generation is stricter because unexplored chunks must use the same algorithm as
existing terrain. A frozen SHA-256 digest covers the entry module, package/dependency
metadata and every server/shared source module in the transitive package closure.
Dynamic imports prevent safely narrowing this to imports observed in one run.
Consequently even an unrelated server helper inside that closure requires a fresh
save. Client-only source and presentation assets are excluded from the generation
digest, though catalog-bound assets still have their own content contracts.
Native Rust contributors have no source digest and must bump their explicit
revision when their algorithm changes.

## Reading a rejection

Content errors identify the kind, namespaced key, namespace, saved numeric ID,
and saved/current contract hashes. `content.map` stores only those hashes, so
an error cannot identify the particular field that changed. Compare the declared
schema, revision, layout, content definition and dependency metadata for that key.
Generation errors distinguish added/removed contributors, declared revision
changes and source/dependency SHA changes, showing the affected key and identities.
Client catalog errors similarly identify the missing, extra or differing key.

Validation occurs before compatibility-related save writes and lock-file creation;
incompatible attempts leave existing save files unchanged. Restore the compatible
package definitions or choose a fresh directory. There is no automatic upgrade.

Save metadata is now format **8**, with optional source digests per generation
contributor. `content.map` remains BGCM v2. The default save directory is
**`world-v24/`** (`world-v24-fixture/` with the lifecycle fixture). Older metadata
formats, including existing `world-v23/` saves, are rejected without conversion.

## Regression coverage

[Restart tests](../../src/server/net/tests/script_startup/save_compatibility.rs)
use the production nonblocking listener and an isolated save. Changed behavior
runs after restart while inventory conservation, persistent entity identity and
private state, owner bytes and deadlines remain intact. Explicit contract and
same-revision generation source edits reject without modifying saved bytes.
[Identity tests](../../src/server/script/package/tests/identity.rs) exercise
transitive dependencies, helper extraction, unrelated packages and client-only
generator edits. [Metadata tests](../../src/storage/metadata/tests.rs) cover
bounded decoding and precise mismatch errors.
