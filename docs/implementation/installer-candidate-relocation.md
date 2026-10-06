# Installer candidate relocation verification

The owner prepares an upgrade from a private extracted candidate. Package hooks
verify a second extraction: DEB control scripts use the package manager's control
directory, PKG scripts use their package extraction, and RPM uses the protected
recovery copy. Comparing the complete candidate previously compared these
different storage roots and absolute payload paths. Source inspection shows
that this rejected otherwise matching prepared candidates before installation;
no native installation was performed to establish that impact.

`verify_prepared_installation` now compares candidate identities after expressing
artifact paths and Desktop payload/executable paths relative to each verified
candidate root. Both roots must be absolute; each payload path must be absolute,
strictly beneath its own root, and contain no parent traversal. Relative layout
must remain identical. The current extraction still passes the existing manifest,
checksum, containment and link checks before comparison. The persisted candidate
was verified when preparation created the owner-bound journal; comparison does
not require its original extraction directory to remain present.

The comparison preserves every other candidate field, including source and
workflow identity, manifest and component hashes, build metadata, platform,
Desktop tree hash and Windows installation-relative managed-file mappings.
Preparation's provenance flag is cleared only for comparison with the freshly
verified candidate, as before; the owning journal must still record verified
provenance. Receipt destinations, program backups, environment/account identity
and service inventory retain their existing exact checks.

The repair changes no persisted schema. Candidate cloning also preserves added
fields when future package contracts extend the candidate type. Disposable local
regressions verify matching bytes in two private roots, reject metadata/layout
changes, and reject escaping paths, altered bytes and links. They execute no
candidate, package manager or service and make no network requests. Bundle paths
and Windows managed-file mappings have comparison coverage; native installation
acceptance remains separate.
