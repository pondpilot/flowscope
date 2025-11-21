# Security Policy

## Supported Versions

We release patches for security vulnerabilities for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take the security of FlowScope seriously. If you discover a security vulnerability, please follow these steps:

### 1. Do Not Publicly Disclose

Please do not create a public GitHub issue for security vulnerabilities.

### 2. Contact Us

Send details of the vulnerability to: security@pondpilot.com

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Any suggested fixes (if available)

### 3. Response Timeline

- We will acknowledge receipt within 48 hours
- We will provide an initial assessment within 5 business days
- We will work with you to understand and address the issue
- We will release a fix as soon as possible

### 4. Disclosure

We will coordinate with you on the timing of public disclosure after a fix is available.

## Security Best Practices

When using FlowScope:

1. **Input Validation**: Always validate and sanitize SQL input before passing to FlowScope
2. **Dependency Updates**: Keep FlowScope and its dependencies up to date
3. **Error Handling**: Handle errors gracefully and avoid exposing sensitive information in error messages
4. **WASM Security**: Be aware that WASM runs in the browser sandbox with the same origin policy

## Known Limitations

FlowScope is designed for SQL analysis and visualization. It is not intended for:

- Executing SQL against production databases
- Handling sensitive credentials or connection strings
- Production ETL or data processing

## Security Updates

Security updates will be released as patch versions and announced via:

- GitHub Security Advisories
- Release notes
- Project README

Thank you for helping keep FlowScope secure!
