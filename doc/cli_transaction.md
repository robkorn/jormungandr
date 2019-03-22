# Transaction

Tooling for offline transactino creation

## Builder

Builds a signed transaction.

The process can be split into steps by passing --file parameter. The intermediate state
will be stored in the given file in YAML format or updated if it already exists. When
unfinished, it's useful prevent generation of final transaction by passing --draft flag.

### Usage

```
jormungandr_cli transaction build <options>
```

The options are

FLAGS:
- -d, --draft do not generate final transaction
- -h, --help Prints help information
- -V, --version Prints version information

OPTIONS:
- -c, --change <change> change address. Value taken from inputs and not spent on outputs
or fees will be returned to this address. If not provided, the change will go to treasury.
Must be bech32-encoded ed25519extended_public key.
- -b, --fee-base <fee-base> fee base which will be always added to the transaction
- -a, --fee-per-addr <fee-per-addr> fee which will be added to the transaction for every
input and output
- -f, --file <file> create or update transaction builder state file
- -i, --input <input>... transaction input. Must have format
`<hex-encoded-transaction-id>:<output-index>:<value>`. E.g. `1234567890abcdef:2:535`.
At least 1 value required.
- -o, --output <output>... transaction output. Must have format `<address>:<value>`.
E.g. `ed25519extended_public1abcdef1234567890:501`. The address must be bech32-encoded
ed25519extended_public key. At least 1 value required.
- -s, --spending-key <spending-key>... file with transaction spending keys. Must be
bech32-encoded ed25519extended_secret. Required one for every input.

Value outputted to stdout on success is binary blob with transaction
