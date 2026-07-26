# Vendored upstream

- Repository: https://github.com/PatWie/drafft-ink
- Revision: `8ce40ab2cf3cde7efa78a7e077fc9267fd4b3761`
- License: `AGPL-3.0`
- Vendored path: `vendor/drafft-ink`
- Local source changes: none

The repository was copied verbatim except for its `.git` directory. Its
workspace manifest, lockfile, license, assets, application sources, core crate,
and renderer crate are retained so the source and provenance stay inspectable.

The integration crate repeats upstream's Vello `[patch.crates-io]` entry because
Cargo ignores patches declared by dependencies. This preserves upstream's
Vello revision and wgpu 27 dependency graph without modifying vendored files.

To update it, review the upstream diff and license first, replace the complete
vendor directory from one immutable commit, update the revision above, then run
the isolated core and Vello checks. Do not merge upstream code into the GPUI
adapter by hand.
