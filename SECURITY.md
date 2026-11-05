# Security policy

Kitty Ledger moves token balances, so please report vulnerabilities privately
through GitHub's "Report a vulnerability" (Security tab) instead of a public issue.

Status: the contracts are unaudited and intended for testnet pilots. Do not use them
with real funds until an independent audit is published. Areas of most interest:
committee approval and threshold logic, refund exactness, the biller registry's
24 h address-change delay, and storage/TTL handling in the campaign contract.
