---
title: "Developing with Rust"
---

Solana supports writing on-chain programs using the
[Rust](https://www.rust-lang.org/) programming language.

## Project Layout

Solana Rust programs follow the typical [Rust project
layout](https://doc.rust-lang.org/cargo/guide/project-layout.html):

```
/inc/
/src/
/Cargo.toml
```

But must also include:

```
/Xargo.toml
```

Which must contain:

```
[target.bpfel-unknown-unknown.dependencies.std]
features = []
```

Solana Rust programs may depend directly on each other in order to gain access
to instruction helpers when making [cross-program invocations](developing/programming-model/calling-between-programs.md#cross-program-invocations).
When doing so it's important to not pull in the dependent program's entrypoint
symbols because they may conflict with the program's own. To avoid this,
programs should define an `exclude_entrypoint` feature in `Cargo.toml` and use
to exclude the entrypoint.

- [Define the
  feature](https://github.com/solana-labs/solana-program-library/blob/a5babd6cbea0d3f29d8c57d2ecbbd2a2bd59c8a9/token/program/Cargo.toml#L12)
- [Exclude the
  entrypoint](https://github.com/solana-labs/solana-program-library/blob/a5babd6cbea0d3f29d8c57d2ecbbd2a2bd59c8a9/token/program/src/lib.rs#L12)

Then when other programs include this program as a dependency, they should do so
using the `exclude_entrypoint` feature.

- [Include without
  entrypoint](https://github.com/solana-labs/solana-program-library/blob/a5babd6cbea0d3f29d8c57d2ecbbd2a2bd59c8a9/token-swap/program/Cargo.toml#L19)

## Project Dependencies

At a minimum, Solana Rust programs must pull in the
[solana-program](https://crates.io/crates/solana-program) crate.

Solana BPF programs have some [restrictions](#restrictions) that may prevent the
inclusion of some crates as dependencies or require special handling.

For example:

- Crates that require the architecture be a subset of the ones supported by the
  official toolchain. There is no workaround for this unless that crate is
  forked and BPF added to that those architecture checks.
- Crates may depend on `rand` which is not supported in Solana's deterministic
  program environment. To include a `rand` dependent crate refer to [Depending
  on Rand](#depending-on-rand).
- Crates may overflow the stack even if the stack overflowing code isn't
  included in the program itself. For more information refer to
  [Stack](overview.md#stack).

## How to Build

First setup the environment:

- Install the latest Rust stable from https://rustup.rs/
- Install the latest Solana command-line tools from
  https://docs.solana.com/cli/install-solana-cli-tools

The normal cargo build is available for building programs against your host
machine which can be used for unit testing:

```bash
$ cargo build
```

To build a specific program, such as SPL Token, for the Solana BPF target which
can be deployed to the cluster:

```bash
$ cd <the program directory>
$ cargo build-bpf
```

## How to Test

Solana programs can be unit tested via the traditional `cargo test` mechanism by
exercising program functions directly.

To help facilitate testing in an environment that more closely matches a live
cluster, developers can use the
[`program-test`](https://crates.io/crates/solana-program-test) crate. The
`program-test` crate starts up a local instance of the runtime and allows tests
to send multiple transactions while keeping state for the duration of the test.

For more information the [test in sysvar
example](https://github.com/solana-labs/solana-program-library/blob/master/examples/rust/sysvar/tests/functional.rs)
shows how an instruction containing syavar account is sent and processed by the
program.

## Program Entrypoint

Programs export a known entrypoint symbol which the Solana runtime looks up and
calls when invoking a program. Solana supports multiple [versions of the BPF
loader](overview.md#versions) and the entrypoints may vary between them.
Programs must be written for and deployed to the same loader. For more details
see the [overview](overview#loaders).

Currently there are two supported loaders [BPF
Loader](https://github.com/solana-labs/solana/blob/7ddf10e602d2ed87a9e3737aa8c32f1db9f909d8/sdk/program/src/bpf_loader.rs#L17)
and [BPF loader
deprecated](https://github.com/solana-labs/solana/blob/7ddf10e602d2ed87a9e3737aa8c32f1db9f909d8/sdk/program/src/bpf_loader_deprecated.rs#L14)

They both have the same raw entrypoint definition, the following is the raw
symbol that the runtime looks up and calls:

```rust
#[no_mangle]
pub unsafe extern "C" fn entrypoint(input: *mut u8) -> u64;
```

This entrypoint takes a generic byte array which contains the serialized program
parameters (program id, accounts, instruction data, etc...). To deserialize the
parameters each loader contains its own wrapper macro that exports the raw
entrypoint, deserializes the parameters, calls a user defined instruction
processing function, and returns the results.

You can find the entrypoint macros here:

- [BPF Loader's entrypoint
  macro](https://github.com/solana-labs/solana/blob/7ddf10e602d2ed87a9e3737aa8c32f1db9f909d8/sdk/program/src/entrypoint.rs#L46)
- [BPF Loader deprecated's entrypoint
  macro](https://github.com/solana-labs/solana/blob/7ddf10e602d2ed87a9e3737aa8c32f1db9f909d8/sdk/program/src/entrypoint_deprecated.rs#L37)

The program defined instruction processing function that the entrypoint macros
call must be of this form:

```rust
pub type ProcessInstruction =
    fn(program_id: &Pubkey, accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult;
```

Refer to [helloworld's use of the
entrypoint](https://github.com/solana-labs/example-helloworld/blob/c1a7247d87cd045f574ed49aec5d160aefc45cf2/src/program-rust/src/lib.rs#L15)
as an example of how things fit together.

### Parameter Deserialization

Each loader provides a helper function that deserializes the program's input
parameters into Rust types. The entrypoint macros automatically calls the
deserialization helper:

- [BPF Loader
  deserialization](https://github.com/solana-labs/solana/blob/7ddf10e602d2ed87a9e3737aa8c32f1db9f909d8/sdk/program/src/entrypoint.rs#L104)
- [BPF Loader deprecated
  deserialization](https://github.com/solana-labs/solana/blob/7ddf10e602d2ed87a9e3737aa8c32f1db9f909d8/sdk/program/src/entrypoint_deprecated.rs#L56)

Some programs may want to perform deserialization themselves and they can by
providing their own implementation of the [raw entrypoint](#program-entrypoint).
Take note that the provided deserialization functions retain references back to
the serialized byte array for variables that the program is allowed to modify
(lamports, account data). The reason for this is that upon return the loader
will read those modifications so they may be committed. If a program implements
their own deserialization function they need to ensure that any modifications
the program wishes to commit be written back into the input byte array.

Details on how the loader serializes the program inputs can be found in the
[Input Parameter Serialization](overview.md#input-parameter-serialization) docs.

### Data Types

The loader's entrypoint macros call the program defined instruction processor
function with the following parameters:

```rust
program_id: &Pubkey,
accounts: &[AccountInfo],
instruction_data: &[u8]
```

The program id is the public key of the currently executing program.

The accounts is an ordered slice of the accounts referenced by the instruction
and represented as an
[AccountInfo](https://github.com/solana-labs/solana/blob/7ddf10e602d2ed87a9e3737aa8c32f1db9f909d8/sdk/program/src/account_info.rs#L10)
structures. An account's place in the array signifies its meaning, for example,
when transferring lamports an instruction may define the first account as the
source and the second as the destination.

The members of the `AccountInfo` structure are read-only except for `lamports`
and `data`. Both may be modified by the program in accordance with the [runtime
enforcement policy](developing/programming-model/accounts.md#policy). Both of
these members are protected by the Rust `RefCell` construct, so they must be
borrowed to read or write to them. The reason for this is they both point back
to the original input byte array, but there may be multiple entries in the
accounts slice that point to the same account. Using `RefCell` ensures that the
program does not accidentally perform overlapping read/writes to the same
underlying data via multiple `AccountInfo` structures. If a program implements
their own deserialization function care should be taken to handle duplicate
accounts appropriately.

The instruction data is the general purpose byte array from the [instruction's
instruction data](developing/programming-model/transactions.md#instruction-data)
being processed.

## Heap

Rust programs implement the heap directly by defining a custom
[`global_allocator`](https://github.com/solana-labs/solana/blob/8330123861a719cd7a79af0544617896e7f00ce3/sdk/program/src/entrypoint.rs#L50)

Programs may implement their own `global_allocator` based on its specific needs.
Refer to the [custom heap example](#examples) for more information.

## Restrictions

On-chain Rust programs support most of Rust's libstd, libcore, and liballoc, as
well as many 3rd party crates.

There are some limitations since these programs run in a resource-constrained,
single-threaded environment, and must be deterministic:

- No access to
  - `rand`
  - `std::fs`
  - `std::net`
  - `std::os`
  - `std::future`
  - `std::process`
  - `std::sync`
  - `std::task`
  - `std::thread`
  - `std::time`
- Limited access to:
  - `std::hash`
  - `std::os`
- Bincode is extremely computationally expensive in both cycles and call depth
  and should be avoided
- String formatting should be avoided since it is also computationally
  expensive.
- No support for `println!`, `print!`, the Solana [logging helpers](#logging)
  should be used instead.
- The runtime enforces a limit on the number of instructions a program can
  execute during the processing of one instruction. See
  [computation budget](developing/programming-model/runtime.md#compute-budget) for more
  information.

## Depending on Rand

Programs are constrained to run deterministically, so random numbers are not
available. Sometimes a program may depend on a crate that depends itself on
`rand` even if the program does not use any of the random number functionality.
If a program depends on `rand`, the compilation will fail because there is no
`get-random` support for Solana. The error will typically look like this:

```
error: target is not supported, for more information see: https://docs.rs/getrandom/#unsupported-targets
   --> /Users/jack/.cargo/registry/src/github.com-1ecc6299db9ec823/getrandom-0.1.14/src/lib.rs:257:9
    |
257 | /         compile_error!("\
258 | |             target is not supported, for more information see: \
259 | |             https://docs.rs/getrandom/#unsupported-targets\
260 | |         ");
    | |___________^
```

To work around this dependency issue, add the following dependency to the
program's `Cargo.toml`:

```
getrandom = { version = "0.1.14", features = ["dummy"] }
```

or if the dependency is on getrandom v0.2 add:

```
getrandom = { version = "0.2.2", features = ["custom"] }
```

## Logging

Rust's `println!` macro is computationally expensive and not supported. Instead
the helper macro
[`msg!`](https://github.com/solana-labs/solana/blob/6705b5a98c076ac08f3991bb8a6f9fcb280bf51e/sdk/program/src/log.rs#L33)
is provided.

`msg!` has two forms:

```rust
msg!("A string");
```

or

```rust
msg!(0_64, 1_64, 2_64, 3_64, 4_64);
```

Both forms output the results to the program logs. If a program so wishes they
can emulate `println!` by using `format!`:

```rust
msg!("Some variable: {:?}", variable);
```

The [debugging](debugging.md#logging) section has more information about working
with program logs the [Rust examples](#examples) contains a logging example.

## Panicking

Rust's `panic!`, `assert!`, and internal panic results are printed to the
[program logs](debugging.md#logging) by default.

```
INFO  solana_runtime::message_processor] Finalized account CGLhHSuWsp1gT4B7MY2KACqp9RUwQRhcUFfVSuxpSajZ
INFO  solana_runtime::message_processor] Call BPF program CGLhHSuWsp1gT4B7MY2KACqp9RUwQRhcUFfVSuxpSajZ
INFO  solana_runtime::message_processor] Program log: Panicked at: 'assertion failed: `(left == right)`
      left: `1`,
     right: `2`', rust/panic/src/lib.rs:22:5
INFO  solana_runtime::message_processor] BPF program consumed 5453 of 200000 units
INFO  solana_runtime::message_processor] BPF program CGLhHSuWsp1gT4B7MY2KACqp9RUwQRhcUFfVSuxpSajZ failed: BPF program panicked
```

### Custom Panic Handler

Programs can override the default panic handler by providing their own
implementation.

First define the `custom-panic` feature in the program's `Cargo.toml`

```toml
[features]
default = ["custom-panic"]
custom-panic = []
```

Then provide a custom implementation of the panic handler:

```rust
#[cfg(all(feature = "custom-panic", target_arch = "bpf"))]
#[no_mangle]
fn custom_panic(info: &core::panic::PanicInfo<'_>) {
    solana_program::msg!("program custom panic enabled");
    solana_program::msg!("{}", info);
}
```

In the above snippit, the default implementation is shown, but developers may
replace that with something that better suits their needs.

One of the side effects of supporting full panic messages by default is that
programs incur the cost of pulling in more of Rust's `libstd` implementation
into program's shared object. Typical programs will already be pulling in a
fair amount of `libstd` and may not notice much of an increase in the shared
object size. But programs that explicitly attempt to be very small by avoiding
`libstd` may take a significant impact (~25kb). To eliminate that impact,
programs can provide their own custom panic handler with an empty
implementation.

```rust
#[cfg(all(feature = "custom-panic", target_arch = "bpf"))]
#[no_mangle]
fn custom_panic(info: &core::panic::PanicInfo<'_>) {
    // Do nothing to save space
}
```

## Compute Budget

Use the system call [`sol_remaining_compute_units`](invalid_link) to return a `u64` indicating the compute units remaining for the program to consume.

Use the system call
[`sol_log_compute_units()`](https://github.com/solana-labs/solana/blob/d3a3a7548c857f26ec2cb10e270da72d373020ec/sdk/program/src/log.rs#L102)
to log a message containing the remaining number of compute units the program
may consume before execution is halted

See [compute budget](developing/programming-model/runtime.md#compute-budget)
for more information.

## ELF Dump

The BPF shared object internals can be dumped to a text file to gain more
insight into a program's composition and what it may be doing at runtime. The
dump will contain both the ELF information as well as a list of all the symbols
and the instructions that implement them. Some of the BPF loader's error log
messages will reference specific instruction numbers where the error occurred.
These references can be looked up in the ELF dump to identify the offending
instruction and its context.

To create a dump file:

```bash
$ cd <program directory>
$ cargo build-bpf --dump
```

## Examples

The [Solana Program Library
github](https://github.com/solana-labs/solana-program-library/tree/master/examples/rust)
repo contains a collection of Rust examples.
