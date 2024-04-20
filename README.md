# forge-flamegraph

a foundry plugin that generates interactive flamegraph for a test case. currently two backends are supported: calltrace and debugtrace.

lets forge some flamegraphs!

[![flamegraph of poseidon2 hash function](./flamegraph_poseidon_debug.svg)](https://zemse.github.io/forge-flamegraph/flamegraph_poseidon_debug.svg)

above is a debugtrace flamegraph of [poseidon2 hash function](https://github.com/zemse/poseidon2).

## Installation

```bash
cargo install --git https://github.com/zemse/forge-flamegraph 
```

## Usage

### `calltrace`

suitable for complex contracts like defi protocols. generates flamegraph svg with the call trace.

```bash
forge-flamegraph -t NAME_OF_TEST_FUNCTION --open
```

### `debugtrace`

suitable for libraries. generates flamegraph svg including solidity internal functions.

```bash
forge-flamegraph -t NAME_OF_TEST_FUNCTION --debugtrace --open
```

> Note: source mappings from the solidity compiler aren't that great, this plugin still tries to guess by looking at source mappings of adjacent steps but unfortunately it only works to some extent.

## Acknowledgements

- [brockelmore](https://github.com/brockelmore) for foundry's debugger
- jonhoo for [inferno](https://github.com/jonhoo/inferno)

and ofcourse for so much oss used in this project