# kcc20-live

My KCC20 demo from the live session.

I ran into errors while coding live. The first commit intentionally keeps the broken code and original comments.

To try it:

```sh
cargo run --locked --bin kcc20
```

To check the contract with the session's compiler version:

```sh
cargo install --git https://github.com/argent-lang/argent --rev a98debf14246294aa3936873d9da82484fa9e764 --locked argent
argentc build contracts/kcc20.ag --out build
```
