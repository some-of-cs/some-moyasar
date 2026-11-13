# Security Policy

## Reporting a vulnerability

Do not disclose suspected vulnerabilities in public issues. Report them privately
through the repository's GitHub security advisory or private vulnerability
reporting channel, including the affected version, a minimal reproduction, and
the security impact.

Do not include real API keys, PANs, CVCs, customer data, or production URLs in
reports. Use test credentials and redacted fixtures.

## Scope and responsibility

This crate is a technical API client. Applications remain responsible for
protecting credentials and payment data, preventing request-body and error
logging, validating deployment-specific privacy and payment obligations, and
maintaining supported dependency versions. The crate does not constitute PCI
DSS, privacy, AML/KYC, sanctions, or other regulatory certification.
