# Release process

1. Confirm `CHANGELOG.md` contains the release date and version.
2. Set the same version in `Cargo.toml`.
3. Run:

   ```console
   cargo fmt --check
   cargo clippy --locked --all-targets -- -D warnings
   cargo test --locked --release
   ```

4. Commit the release preparation.
5. Create and push a signed or annotated tag:

   ```console
   git tag -a v0.1.0 -m "Viewlume 0.1.0"
   git push origin main v0.1.0
   ```

6. The release workflow builds Linux and Windows archives, creates SHA-256 checksum files, and publishes a GitHub Release.
7. Download both archives and verify that Viewlume starts before announcing the release.

The workflow also publishes AppImage, DEB, MSI, SPDX SBOM, and GitHub build
provenance artifacts. To Authenticode-sign the Windows executable and MSI, set
the `WINDOWS_CERTIFICATE_BASE64` repository secret to a base64-encoded PFX and
`WINDOWS_CERTIFICATE_PASSWORD` to its password. Unsigned artifacts are produced
when these secrets are absent.

Do not reuse or move an existing release tag.
