# Backend-controlled self-update provenance

Status: accepted. The frontend expresses update intent, but the backend owns Release Asset Provenance and resolves assets from the official release. The first remediation phase uses fixed repository/tag/asset validation, redirect and size limits, and official checksums; release signatures are a later phase because key management is not yet established.

## Considered Options

- Trust frontend-supplied HTTPS URLs: rejected because an injected UI could select an attacker-controlled executable and matching checksum.
- Require release signatures immediately: deferred because it introduces key storage, rotation, and recovery work that is not currently present in the release process.

## Consequences

The first phase reduces the IPC and transport attack surface but does not protect against compromise of the official repository or checksum manifest. The signature phase must preserve the backend-controlled asset contract rather than reintroduce frontend URL trust.
