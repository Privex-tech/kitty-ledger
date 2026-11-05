# Changelog

## Unreleased

### Added
- CLI registry-admin commands: `biller-register`, `biller-verify`, `biller-active`,
  `biller-propose-address`, `biller-apply-address`, `biller-cancel-address`, `biller-status`.
- GitHub Actions CI (contract tests and CLI tests), issue and pull request templates,
  Dependabot config.
- `LICENSE` (Apache-2.0), `CONTRIBUTING.md`, `SECURITY.md`, `.gitignore`.

### Validated
- Full campaign and refund flow executed on Stellar testnet; hashes in `VALIDATION.md`, ledger export in `data/validation/`.

### Fixed
- Documentation referenced files that do not exist and used a wrong quickstart path.

## 0.1.0
- `biller_registry` and `campaign` Soroban contracts, `kitty` CLI, public ledger page,
  seed data and offline test suites.
