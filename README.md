# Requirements

1. Bitcoin Core, with ZMQ support. This needs be be running with the following
   flags:

   ```
   user=user
   password=password
   signetblocktime=60
   signetchallenge=00141f61d57873d70d28bd28b3c9f9d6bf818b5a0d6a
   acceptnonstdtxn=1 # Important! Otherwise Core rejects OP_DRIVECHAIN TXs

   # this can also be set to a different address, as long
   # as you set the CLI arg for bip300301_enforcer
   zmqpubsequence=tcp://0.0.0.0:29000
   txindex=1
   ```

1. Rustc & Cargo, version 1.78.0 or higher. Installing via Rustup is
   recommended.

# Getting started

Building/running:

```bash
# Check out git submodules
$ git submodule update --init --recursive
# Compiles the project
$ cargo build

# See available options
$ cargo run -- --help

# Starts the gRPC server at localhost:50001
# Adjust these parameters to match your local Bitcoin
# Core instance
$ cargo run -- \
  --node-rpc-addr=localhost:38332 \
  --node-rpc-user=user \
  --node-rpc-pass=password \
  --node-zmq-addr-sequence=tcp://0.0.0.0:29000

# You should now be able to fetch data from the server!
$ buf curl  --http2-prior-knowledge --protocol grpc \
        http://localhost:50051/cusf.validator.v1.ValidatorService/GetChainInfo
{
  "network": "NETWORK_SIGNET"
}
```

# Interacting with the enforcer

The CUSF enforcer exposes multiple gRPC services. These can be interacted with
using a gRPC client of your choice, for example
[`buf curl`](https://buf.build/docs/installation/) or
[`grpcurl`](https://github.com/fullstorydev/grpcurl).

Some examples of interacting with the enforcer using `buf curl`, assuming you
expose the server at the default address `localhost:50051`:

```bash
# Define an alias for ease of use
$ alias buf_curl='buf curl --http2-prior-knowledge --protocol grpc --emit-defaults'

# List all the available RPCs
$ buf_curl --list-methods http://localhost:50051
cusf.mainchain.v1.ValidatorService/GetBlockHeaderInfo
cusf.mainchain.v1.ValidatorService/GetChainInfo
cusf.mainchain.v1.ValidatorService/GetChainTip
cusf.mainchain.v1.ValidatorService/GetSidechains
cusf.mainchain.v1.WalletService/CreateNewAddress
cusf.mainchain.v1.WalletService/CreateSidechainProposal
... list continues

# Fetching data with a RPC that takes no input data
$ buf_curl http://localhost:50051/cusf.mainchain.v1.ValidatorService/GetChainInfo
{
  "network": "NETWORK_SIGNET"
}

# Fetching data with a RPC that takes input data
$ request='{"block_hash": {"hex": "000002a78fc54150bb2d4cdb0fb19bcf744f2877faf90a172972fca5daf5fe92"}}'
$ buf_curl -d "$request" http://localhost:50051/cusf.mainchain.v1.ValidatorService/GetBlockHeaderInfo
{
  "headerInfo": {
    "blockHash": {
      "hex": "000002a78fc54150bb2d4cdb0fb19bcf744f2877faf90a172972fca5daf5fe92"
    },
    "prevBlockHash": {
      "hex": "000002501d569e62a56ea175896d4348dd9cfef1d700e5b06250486df07c9225"
    },
    "height": 34998,
    "work": {
      "hex": "14d4490000000000000000000000000000000000000000000000000000000000"
    }
  }
}

# Note that the request can also be read from a file. This can come in handy if
# you're working on more complex requests that's hard to write out on the terminal
echo "$request" > request.json
$ buf_curl -d @request.json http://localhost:50051/cusf.mainchain.v1.ValidatorService/GetBlockHeaderInfo
```

# Regtest

By default, the enforcer runs against our custom signet. If you instead want to
run against a local regtest, you need to also run a local regtest Electrum
server. There are multiple implementations of Electrum servers, an easy-to-use
one is [`mempool/electrs`](https://github.com/mempool/electrs).

For complete instructions on how to do this, consult the
[official docs](https://github.com/mempool/electrs).

A quickstart (that might not work, in case you're missing some dependencies):

```bash
$ git clone https://github.com/mempool/electrs

$ cd electrs

$ cargo run --bin electrs --release -- \
    --network regtest \
    --cookie=user:password \
    --jsonrpc-import
```

# Logging

The application uses the `tracing` crate for logging. Logging is configured
through setting the `--log-level` argument. Some examples:

```bash
# Prints ALL debug logs
$ cargo run ... --log-level DEBUG
```

Logs can also be configured via env vars, which take precedence over CLI args.

```bash
# Prints logs at the "info" level and above, plus our logs the "debug" level and above
$ RUST_LOG=info,bip300301_enforcer=debug cargo run ...
```

# Working with the proto files

Code is generated with [protox](https://github.com/andrewhickman/protox), and
happens automatically as part of the build process.

Files are linted with [protolint](https://github.com/yoheimuta/protolint).

To lint the files, run:

```bash
$ protolint lint --fix proto/validator/v1/validator.proto
```

# Code formatting

Rust code is formatted with [rustfmt](https://github.com/rust-lang/rustfmt).

Markdown and YAML files are formatted with [Prettier](https://prettier.io/). The
easiest way to run it is to install the
[Prettier VSCode extension](https://marketplace.visualstudio.com/items?itemName=esbenp.prettier-vscode).

To run it from the command line, install the
[Prettier CLI](https://prettier.io/docs/en/cli.html) and run it from the root of
the repo:

```bash
$ prettier --write .
```

# Integration tests

Integration tests can be run using

```bash
$ cargo run --example integration_tests -- <TEST ARGS>
```

# JSON-RPC server

The application provides a JSON-RPC server that runs on `127.0.0.1:8122` by default. The server supports the following methods:

### Available Methods

1. **ping**
   - Description: A simple health check method that returns "pong"
   - Parameters: None
   - Returns: `"pong"`
   - Example:
     ```bash
     curl -X POST -H "Content-Type: application/json" -d '{
       "jsonrpc": "2.0",
       "method": "ping",
       "params": [],
       "id": 1
     }' http://127.0.0.1:8122
     ```

2. **list_sidechain_deposit_transactions**
   - Description: Lists all sidechain deposit transactions from the wallet
   - Parameters: None
   - Returns: Array of deposit transactions with the following structure:
     ```json
     {
       "sidechain_number": number,
       "txid": string,
       "fee_sats": number,
       "received_sats": number,
       "sent_sats": number,
       "confirmation": {
         "height": number,
         "block_hash": string,
         "timestamp": number
       } | null
     }
     ```
   - Example:
     ```bash
     curl -X POST -H "Content-Type: application/json" -d '{
       "jsonrpc": "2.0",
       "method": "list_sidechain_deposit_transactions",
       "params": [],
       "id": 1
     }' http://127.0.0.1:8122
     ```

3. **broadcast_withdrawal_bundle**
   - Description: Broadcasts a withdrawal bundle transaction for a specific sidechain
   - Parameters: 
     1. `sidechain_number` (number): The sidechain number (u8)
     2. `transaction_hex` (string): The hex-encoded withdrawal bundle transaction
   - Returns: Object containing the M6ID of the broadcast withdrawal bundle
     ```json
     {
       "m6id": string
     }
     ```
   - Example:
     ```bash
     curl -X POST -H "Content-Type: application/json" -d '{
       "jsonrpc": "2.0",
       "method": "broadcast_withdrawal_bundle",
       "params": [1, "0200000001abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789000000006a47304402..."],
       "id": 1
     }' http://127.0.0.1:8122
     ```

The server includes built-in request ID tracking, HTTP tracing, and RPC logging for debugging purposes.

