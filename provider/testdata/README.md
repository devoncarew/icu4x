# icu_testdata [![crates.io](https://img.shields.io/crates/v/icu_testdata)](https://crates.io/crates/icu_testdata)

`icu_testdata` is a unit testing package for [`ICU4X`].

The package exposes a `DataProvider` with stable data useful for unit testing. The data is
based on a CLDR tag and a short list of locales that, together, cover a range of scenarios.

The list of locales and the current CLDR tag can be found in [Cargo.toml](./Cargo.toml).

The output data can be found in the [data](./data/) subdirectory. There, you will find:

- `json` for the ICU4X JSON test data
- `cldr` for the source CLDR JSON

### Pointing to custom test data

If you wish to run ICU4X tests with custom test data, you may do so by setting the "ICU4X_TESTDATA_DIR" environment variable:

```bash
$ ICU4X_TESTDATA_DIR=/path/to/custom/testdata cargo test
```

### Re-generating the data

From the top level directory of the `icu4x` metapackage, run:

```bash
$ cargo make testdata
```

The following commands are also available:

- `cargo make testdata-download` downloads fresh CLDR JSON
- `cargo make testdata-build-json` re-generates the ICU4X JSON
- `cargo make testdata-build-blob` re-generates the ICU4X blob file
- `cargo make testdata-build-bincode` re-generates Bincode filesystem testdata

## Examples

```rust
use std::borrow::Cow;
use icu_provider::prelude::*;
use icu_locid::locale;

let data_provider = icu_testdata::get_provider();

let data: DataPayload<icu_plurals::provider::CardinalV1Marker> = data_provider
    .load_resource(&DataRequest {
        options: locale!("ru").into(),
        metadata: Default::default(),
    })
    .unwrap()
    .take_payload()
    .unwrap();
let rule = "v = 0 and i % 10 = 2..4 and i % 100 != 12..14".parse()
    .expect("Failed to parse plural rule");
assert_eq!(data.get().few, Some(rule));
```

[`ICU4X`]: ../icu/index.html

## More Information

For more information on development, authorship, contributing etc. please visit [`ICU4X home page`](https://github.com/unicode-org/icu4x).
