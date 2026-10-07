# Sonos public trust certificates

Retrieved 2026-10-07T19:48Z for offline analysis. No speaker connection,
machine trust change, firmware execution, or product edit was performed.

## Source

Archive: `docs-ignore/victrola-hacks.tgz`, member `./sonos-integration.md`,
section "Firmware evidence for repeating the analysis". Archive SHA-256:
`63b2b5ff8d94b28bf91db26a6c82a4b82c072794be0491a162145a8497602ff7`.

Vendor download:
https://victrola-stream-firmware-update.s3.us-west-1.amazonaws.com/0.43.179_image.swu

Downloaded using curl with normal HTTPS certificate verification and HTTPS-only
redirects. SWU SHA-256 matches the archived report exactly:
`9d18aa1b92466d33e215427e6fb6b9d0d3bb64698a074fcc5d837d818d5fb046`.
The SWU signature was not independently verified.

CPIO member `yocto-nsdk-ip-image-swu-stream1832victrola.ubifs` was decompressed
with xz. Decompressed image SHA-256:
`5e7ddca619fcc59a7aa8a756284f86257dfa8b06832516a3c894e6021f587918`.

ubi-reader 0.8.16 read `/usr/lib64/libnsdk_victrolaworker.so` into a pipe.
Only its two BEGIN/END CERTIFICATE PEM blocks were written as extracted files,
in occurrence order. No executable or private key was extracted to a file.
The temporary SWU and UBIFS containers were removed after extraction.

## Certificate identities

`sonos-root-1.pem`: Sonos Device Authentication Root CA, self-issued, CA:TRUE.
Validity: 2015-10-05 through 2055-09-25.
DER SHA-256 fingerprint:
`42DCD4A7D4B23BD28683B403377BA88A169B3578EC94F415BBE6FD483DD5A08F`.

`sonos-root-2.pem`: Sonos Root CA, self-issued, CA:TRUE.
Validity: 2014-04-03 through 2035-07-02.
DER SHA-256 fingerprint:
`A24AB0D60C49F44ABF3689CA7366F0316BF568B413272CFE6A723394F00F562A`.

`sonos-roots.pem` concatenates both public roots. For this captured leaf,
`sonos-root-1.pem` alone suffices; root 2 is not part of its chain.

`captured-leaf.pem` was converted with openssl x509 from the already-captured
`../tls-chain.txt`; it is an untrusted verification input, never a trust anchor.
Original capture SHA-256:
`35e4ef272f5d41e3faba469dff2a35b0732bb585aeabcd46fd9190a5c202b81a`.

## Offline verification

OpenSSL 3.5.5, current system UTC date 2026-10-07. From repository root:

```sh
openssl verify -no-CApath -no-CAstore \
  -CAfile target/sonos/trust/sonos-root-1.pem \
  -purpose sslserver -show_chain -check_ss_sig \
  target/sonos/trust/captured-leaf.pem
```

Exit 0: OK. Chain: captured leaf CN=38420B8F3F0C directly to Sonos Device
Authentication Root CA. No intermediate required. The same check with the
two-root bundle also passed. Full output: `verify-chain.txt`.

Additional strict-profile diagnostic:

```sh
openssl verify -no-CApath -no-CAstore \
  -CAfile target/sonos/trust/sonos-roots.pem \
  -purpose sslserver -show_chain -x509_strict \
  target/sonos/trust/captured-leaf.pem
```

Exit 2: error 85 at depth 0, Missing Authority Key Identifier.
Full output: `verify-strict.txt`. This is a policy-review input, not approval
to alter a client's strictness. No TLS verification setting was changed.

These checks establish certificate-chain validation under the named OpenSSL
profiles only. They do not verify a requested hostname/IP, revocation status,
live possession of the leaf's private key, or application authorization.
The parent owns identity matching and TLS-policy decisions before a live probe.

## PEM file SHA-256 hashes

The checked-in `sonos-roots.pem` bundle has SHA-256
`7c57f74c70c9f91cb54225ef5ec5ef16c1da90a72a0ee6c1f1700760339f3ef8`.
Private extraction records remain under `target/sonos/trust/`; these file hashes
are distinct from the DER fingerprints above.
