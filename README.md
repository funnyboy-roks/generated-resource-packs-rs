# Generated Resource Packs

This is a fairly tailored tool for generating some of my resource packs,
rewritten in Rust.  (previously
[JavaScript](https://github.com/funnyboy-roks/generated-resource-packs))

It will automatically fetch the textures for the latest version and
build a resource pack from them using a function that maps textures.

There is a binary, `poll`, that will automatically poll Mojang's API
daily for new releases and will automatically build the pack and upload
it to Modrinth.  (Hence tailored)

## Usage

### Main binary

The main binary should be pretty generic, run it with

```sh
cargo run --release
```

and it will download the latest version and put the output zips in
`out`.

A version may be specified like

```sh
cargo run --release -- 26.2
```

to build the packs for a specific version.

### `poll`

As said above, this binary is very specific, so these instructions are
more for me.

An environment variable, `MODRINTH_TOKEN`, must be set to the PAT used
for uploading to modrinth (with Create Version scope).

The `Dockerfile` will create an image that can be used to run the
application, or use

```sh
cargo r --release --bin poll
```

to run it directly.
