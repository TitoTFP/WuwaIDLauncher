# WuwaID Launcher Context

This context defines the domain language for managing the game installation, Patch ID lifecycle, launcher updates, and the boundaries between observing and mutating local state.

## Language

**Game Installation**:
The normalized Wuthering Waves root that contains the game executable and the locations where launcher-managed patch artifacts may live.
_Avoid_: game folder, install directory

**Patch Method**:
The selected way to make Patch ID available to the game: `resource_mount` or `loader`.
_Avoid_: mode, strategy, installer type

**Canonical Artifact**:
A file or mount entry at a path reserved by the launcher for a Patch Method, including artifacts written by an older launcher version.
_Avoid_: mod file, random patch file

**Legacy Artifact**:
A Canonical Artifact left by an older launcher contract that may lack current metadata or an ownership marker.
_Avoid_: foreign artifact, old file

**Observation**:
A non-mutating read of game, patch, media, or launcher state whose result may become stale while another operation runs.
_Avoid_: mutation, operation

**Mutation**:
A launcher operation that changes managed files, settings, cache contents, process lifecycle, or the current launcher executable.
_Avoid_: action, command

**Operation Lock**:
The launcher-wide rule that allows only compatible Mutations to run at the same time and rejects conflicting work as busy.
_Avoid_: frontend lock, click lock

**Canonical Artifact Ownership**:
The decision rule that determines whether the launcher may delete or replace a Canonical Artifact, including the explicit confirmation path for an artifact without matching ownership proof.
_Avoid_: cleanup permission, file trust

**Release Asset Provenance**:
The evidence that an update asset belongs to the expected official release, including its repository, tag, asset name, redirect destination, size, and checksum; a future phase may add a release signature.
_Avoid_: download URL, checksum alone
