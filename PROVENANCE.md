# Provenance

Everything in this repository is written from scratch for the Metallium-OS greeter.

| Source | What was taken | How |
|---|---|---|
| [metallium-os](https://github.com/anthonydarkoonx/metallium-os) (Apache-2.0 OR MIT, same author) | The greeter contract (`docs/GREETER-INTERFACE.md`), the data store's lock protocol (`proto::volume` in `libs/metallium-driver-rt`), the session service's protocol (`programs/src/bin/sessiond.rs`), the input and display driver protocols, the 5x9 bitmap font (`libs/metallium-gfx`) | Interfaces as documented; `metallium-driver-rt` and `metallium-gfx` are git dependencies |
| Crockford's base 32 alphabet (public domain description) | The recovery key's character set (no I, L, O, U; look-alikes read as digits) | The idea only; the encoder and decoder are written here |
