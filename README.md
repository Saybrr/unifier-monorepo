# Unifier Repo

Monorepo to host the Unifier CLI, Desktop Client, and shared crates
```
unifier/
├── Cargo.toml              # Workspace manifest
├── Cargo.lock              # Shared lockfile
├── apps/
│   ├── unifier-client/     # Tauri frontend + backend
│   │   ├── src-tauri/      # Rust backend for Tauri
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   └── src/ui          # Typescript Frontend
│   └── cli/                # CLI tool
│       ├── Cargo.toml
│       └── src/
├── crates/
│   └── installer/               # Shared Installer Logic
│       ├── Cargo.toml
│       └── src/
└── target/                 # Build artifacts (ignored in git)
```

## Running
- using Developer Powershell for Visual Studio 2022
- CLI:  `cargo run -p cli`
  - Command inputs can be given by `...cli -- --input "hello"`
- Unifier Client: from `apps/unifier-client` - `awesome-app dev`


## Testing
- Unit Tests should go in the same file as the functions they are testing [Rust Book Src](https://rust-book.cs.brown.edu/ch11-03-test-organization.html#unit-tests)
- Integration Tests will go in a Tests directory inside the relevant module
- End to End tests will go in the Tests directory of the workspace root, run with `cargo test --workspace`

See Developer_Onboarding.md for an overview of the Tauri Template
- Additional Info for the tauri Template is available here:
- https://awesomeapp.dev/
https://m.youtube.com/watch?v=BY_ZjPGqJJk
https://github.com/awesomeapp-dev/rust-desktop-app