# WinGet submission

The `manifests` directory mirrors the path required by the
`microsoft/winget-pkgs` community repository. Before submitting a release:

1. Publish the matching immutable GitHub release asset.
2. Update the version, release URL, and SHA-256 hash in all manifest files.
3. Run `winget validate --manifest .\winget\manifests\f\Fresh600\NTIV\<version>`.
4. Test installation on a clean Windows 11 x64 environment.
5. Submit exactly one package version per pull request.
