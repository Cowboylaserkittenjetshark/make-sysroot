# make-sysroot
This is a CLI tool designed to generate a sysroot from the filesystem of a cross compilation target

Heavily based on Marcus Behel's [RustCrossExperiments](https://github.com/MB3hel/RustCrossExperiments/blob/76933201f80aec397bc37eadfcdbaacac5da109e/make-sysroot.sh)

## Config file
The config file specifies what files get copied to and created in the destination directory.

By default, the config file is assumed to be `make-sysroot.toml` in the current working directory. To override this, pass the path to your config file with the `--config` flag.
**Fields:**
- `include`: paths to include in the sysroot
- `exclude`: paths to exclude from the sysroot
- `link`: symlinks to create within the sysroot
  - `link`: the path to the link
  - `target`: the path the link points to

All paths specified in the config file should be absolute paths, relative to the sysroot:

  If the real path to the file on your system is `/mnt/usr/lib/thingy`, to include it, you would specify `/usr/lib/thingy` in the includs section of make-sysroot.toml

An example config file is located in the `examples` directory.
