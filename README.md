# metallium-greeter

The greeter for [Metallium-OS](https://github.com/anthonydarkoonx/metallium-os): the screen shown before a session starts. It unlocks the encrypted data volume, sets up the
first account on a new machine, logs users in, and replaces a forgotten passphrase with the recovery key.

| Crate | What it is | State |
|---|---|---|
| `crates/greeter-core` | The screens as a state machine: keys in, a description of the screen out, and a `Backend` trait for what the system does (unlock, make an account, log in, recover). No system calls, no drawing | done, host-tested (10 tests) |
| `crates/metallium-greeter-boot` | The OS side of the greeter contract (`metallium-os docs/GREETER-INTERFACE.md`): `metallium_greeter_run`. Draws with the OS's 5x9 font, reads the keyboard driver, talks to the data store and the session service | done, tested end to end under QEMU |

## The screens
* **Unlock**: the data volume is encrypted and locked (a machine whose own secret does not open it): type its passphrase.
* **Welcome** (first boot, no accounts): user name, passphrase (at least 8 characters), again. Makes the **administrator** and shows the **recovery key** once: 26 characters in groups of
  four, to be written on paper. To go on you type its last group back, so it is read, not clicked past. Nothing keeps the key: it is the only way back into the account's files if the
  passphrase is forgotten.
* **Log in**: user name, passphrase. A wrong name and a wrong passphrase give the same answer. A user who already has a session on another terminal is told so.
* **Recovery** (Ctrl+R on the login screen): user name, recovery key (case, dashes and spaces do not matter; `O` reads as `0`, `I` and `L` as `1`), a new passphrase twice.
* **Damaged volume**: an encrypted volume whose store does not read is never formatted over; the screen says so and stops.

## Building into Metallium-OS
```sh
cd ../metallium-os
make run-window GREETER=../metallium-greeter        # or any target that builds the user image; combine with DESKTOP=
make greeter-test GREETER=../metallium-greeter      # the end-to-end test under QEMU
```
`make greeter-lib` builds this repository's `metallium-greeter-boot` for the OS target with the OS's own `metallium-driver-rt` and `metallium-gfx` patched in. The `log-key` feature
(used only by the test) prints a new account's recovery key on the console so a harness that cannot read the screen can type it back: **never build an image with it**.

```sh
cargo test -p greeter-core        # the host tests
cargo clippy --workspace --all-targets
```

## Notes
* The session service (`sessiond`) is not part of the boot flow yet: the OS only passes its endpoint to the greeter in the test mode, so on a normal boot today the greeter only
  unlocks the volume. Wiring `sessiond` into the boot flow (and the desktop per session) is the next step of `metallium-os docs/MULTIUSER-PLAN.md`.
* Licensed Apache-2.0 OR MIT, like Metallium-OS. See `PROVENANCE.md`.
