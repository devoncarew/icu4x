// This file is part of ICU4X. For terms of use, please see the file
// called LICENSE at the top level of the ICU4X source tree
// (online at: https://github.com/unicode-org/icu4x/blob/main/LICENSE ).

use crate::cldr::cldr_serde;
use crate::cldr::error::Error;
use crate::cldr::reader::{get_langid_subdirectories, get_langid_subdirectory, open_reader};
use crate::cldr::CldrPaths;
use elsa::sync::FrozenBTreeMap;

use icu_datetime::provider::calendar::*;
use icu_locid::{unicode_ext_key, Locale};
use icu_provider::datagen::IterableResourceProvider;
use icu_provider::prelude::*;
use std::convert::TryFrom;
use std::path::PathBuf;
use std::str::FromStr;

mod patterns;
mod skeletons;
mod symbols;
pub mod week_data;

/// Common code for a data provider reading from CLDR JSON dates files.
#[derive(Debug)]
pub struct CommonDateProvider {
    paths: Vec<(&'static str, &'static str, PathBuf)>,
    data: FrozenBTreeMap<ResourceOptions, Box<cldr_serde::ca::Dates>>,
}

impl CommonDateProvider {
    /// Constructs an instance from paths to source data.
    pub fn try_new(cldr_paths: &(impl CldrPaths + ?Sized)) -> eyre::Result<Self> {
        Ok(Self {
            paths: cldr_paths.cldr_dates_all(),
            data: FrozenBTreeMap::new(),
        })
    }
}

impl TryFrom<&crate::DatagenOptions> for CommonDateProvider {
    type Error = eyre::ErrReport;
    fn try_from(options: &crate::DatagenOptions) -> eyre::Result<Self> {
        CommonDateProvider::try_new(
            &**options
                .cldr_paths
                .as_ref()
                .ok_or_else(|| eyre::eyre!("CommonDateProvider requires cldr_paths"))?,
        )
    }
}

macro_rules! impl_resource_provider {
    ($(($marker:ident, $expr:expr)),+) => {
        $(
            impl ResourceProvider<$marker> for CommonDateProvider {
                fn load_resource(
                    &self,
                    req: &DataRequest,
                ) -> Result<DataResponse<$marker>, DataError> {
                    let langid = req.options.get_langid();
                    let calendar = req.options.get_unicode_ext(&unicode_ext_key!("ca"))
                        .ok_or_else(|| DataErrorKind::NeedsVariant.with_req(<$marker>::KEY, req))?
                        .to_string();

                    let dates = if let Some(dates) = self.data.get(&req.options) {
                        dates
                    } else {
                        let (cldr_cal, _, path) = self
                            .paths
                            .iter()
                            .find(|(_, bcp_cal, _)| bcp_cal == &calendar)
                            .ok_or_else(|| DataErrorKind::MissingVariant.with_req(<$marker>::KEY, req))?;

                        let locale_dir = get_langid_subdirectory(&path.join("main"), &langid)?
                            .ok_or_else(|| DataErrorKind::MissingLocale.with_req(<$marker>::KEY, req))?;

                        let cal_file = format!("ca-{}.json", cldr_cal);
                        let path = locale_dir.join(&cal_file);

                        let mut resource: cldr_serde::ca::Resource =
                            serde_json::from_reader(open_reader(&path)?)
                                .map_err(|e| Error::Json(e, Some(path)))?;

                        self.data.insert(req.options.clone(), Box::new(resource
                            .main
                            .0
                            .remove(&langid)
                            .expect("CLDR file contains the expected language")
                            .dates
                            .calendars
                            .remove(*cldr_cal)
                            .ok_or_else(|| {
                                Error::Custom(
                                    format!("{} does not have {} field", cal_file, cldr_cal),
                                    None,
                                )
                        })?))
                    };

                    let metadata = DataResponseMetadata::default();
                    // TODO(#1109): Set metadata.data_langid correctly.
                    Ok(DataResponse {
                        metadata,
                        #[allow(clippy::redundant_closure_call)]
                        payload: Some(DataPayload::from_owned(($expr)(dates, &calendar))),
                    })
                }
            }

            impl IterableResourceProvider<$marker> for CommonDateProvider {
                fn supported_options(
                    &self,
                ) -> Result<Box<dyn Iterator<Item = ResourceOptions> + '_>, DataError> {
                    // Raise an error if Gregorian paths are not available
                    self.paths
                        .iter()
                        .find(|(_, cal, _)| cal == &"gregory")
                        .ok_or(Error::MissingSource(crate::cldr::error::MissingSourceError {
                            src: "cldr-dates/gregory",
                        }))?;

                    let mut r = Vec::new();
                    for (_, cal, path) in &self.paths {
                        let cal_value = icu_locid::extensions::unicode::Value::from_str(cal)
                            .map_err(|e| DataError::custom(
                                "could not parse calendar identifier"
                            ).with_error_context(&e))?;
                        r.extend(
                            get_langid_subdirectories(&path.join("main"))?
                                .map(|lid| {
                                    let mut locale: Locale = lid.into();
                                    locale.extensions.unicode.keywords.set(unicode_ext_key!("ca"), cal_value.clone());
                                    ResourceOptions::from(locale)
                                })
                        );
                    }
                    Ok(Box::new(r.into_iter()))
                }
            }
        )+

        icu_provider::impl_dyn_provider!(CommonDateProvider, [$($marker),+,], SERDE_SE, ITERABLE_SERDE_SE, DATA_CONVERTER);
    };
}

impl_resource_provider!(
    (DateSymbolsV1Marker, symbols::convert_dates),
    (DateSkeletonPatternsV1Marker, |dates, _| {
        DateSkeletonPatternsV1::from(dates)
    }),
    (DatePatternsV1Marker, |dates, _| DatePatternsV1::from(dates))
);

#[cfg(test)]
mod test {
    use super::*;
    use icu_datetime::pattern::runtime::{Pattern, PluralPattern};
    use icu_plurals::PluralCategory;

    #[test]
    fn test_basic_patterns() {
        let cldr_paths = crate::cldr::cldr_paths::for_test();
        let provider =
            CommonDateProvider::try_new(&cldr_paths).expect("Failed to retrieve provider");

        let locale: Locale = "cs-u-ca-gregory".parse().unwrap();
        let cs_dates: DataPayload<DatePatternsV1Marker> = provider
            .load_resource(&DataRequest {
                options: locale.into(),
                metadata: Default::default(),
            })
            .expect("Failed to load payload")
            .take_payload()
            .expect("Failed to retrieve payload");

        assert_eq!("d. M. y", cs_dates.get().date.medium.to_string());
    }

    #[test]
    fn test_with_numbering_system() {
        let cldr_paths = crate::cldr::cldr_paths::for_test();
        let provider =
            CommonDateProvider::try_new(&cldr_paths).expect("Failed to retrieve provider");

        let locale: Locale = "haw-u-ca-gregory".parse().unwrap();
        let cs_dates: DataPayload<DatePatternsV1Marker> = provider
            .load_resource(&DataRequest {
                options: locale.into(),
                metadata: Default::default(),
            })
            .expect("Failed to load payload")
            .take_payload()
            .expect("Failed to retrieve payload");

        assert_eq!("d MMM y", cs_dates.get().date.medium.to_string());
        // TODO(#308): Support numbering system variations. We currently throw them away.
        assert_eq!("d/M/yy", cs_dates.get().date.short.to_string());
    }

    #[test]
    fn test_datetime_skeletons() {
        let cldr_paths = crate::cldr::cldr_paths::for_test();
        let provider =
            CommonDateProvider::try_new(&cldr_paths).expect("Failed to retrieve provider");

        let locale: Locale = "fil-u-ca-gregory".parse().unwrap();
        let skeletons: DataPayload<DateSkeletonPatternsV1Marker> = provider
            .load_resource(&DataRequest {
                options: locale.into(),
                metadata: Default::default(),
            })
            .expect("Failed to load payload")
            .take_payload()
            .expect("Failed to retrieve payload");
        let skeletons = &skeletons.get().0;

        assert_eq!(
            Some(
                &"L".parse::<Pattern>()
                    .expect("Failed to create pattern")
                    .into()
            ),
            skeletons.get(&SkeletonV1::try_from("M").expect("Failed to create Skeleton"))
        );

        let mut expected = PluralPattern::new(
            "'linggo' w 'ng' Y"
                .parse()
                .expect("Failed to create pattern"),
        )
        .expect("Failed to create PatternPlurals");
        expected.maybe_set_variant(
            PluralCategory::One,
            "'ika'-w 'linggo' 'ng' Y"
                .parse()
                .expect("Failed to create pattern"),
        );
        assert_eq!(
            Some(&expected.into()),
            skeletons.get(&SkeletonV1::try_from("yw").expect("Failed to create Skeleton"))
        );
    }

    #[test]
    fn test_basic_symbols() {
        let cldr_paths = crate::cldr::cldr_paths::for_test();
        let provider = CommonDateProvider::try_new(&cldr_paths).unwrap();

        let locale: Locale = "cs-u-ca-gregory".parse().unwrap();
        let cs_dates: DataPayload<DateSymbolsV1Marker> = provider
            .load_resource(&DataRequest {
                options: locale.into(),
                metadata: Default::default(),
            })
            .unwrap()
            .take_payload()
            .unwrap();

        assert_eq!("srpna", cs_dates.get().months.format.wide.0[7]);

        assert_eq!(
            "po",
            cs_dates.get().weekdays.format.short.as_ref().unwrap().0[1]
        );
    }

    #[test]
    fn unalias_contexts() {
        let cldr_paths = crate::cldr::cldr_paths::for_test();
        let provider = CommonDateProvider::try_new(&cldr_paths).unwrap();

        let locale: Locale = "cs-u-ca-gregory".parse().unwrap();
        let cs_dates: DataPayload<DateSymbolsV1Marker> = provider
            .load_resource(&DataRequest {
                options: locale.into(),
                metadata: Default::default(),
            })
            .unwrap()
            .take_payload()
            .unwrap();

        // Czech months are not unaliased because `wide` differs.
        assert!(cs_dates.get().months.stand_alone.is_some());

        // Czech months are not unaliased because `wide` differs.
        assert!(cs_dates
            .get()
            .months
            .stand_alone
            .as_ref()
            .unwrap()
            .abbreviated
            .is_none());
        assert!(cs_dates
            .get()
            .months
            .stand_alone
            .as_ref()
            .unwrap()
            .short
            .is_none());
        assert!(cs_dates
            .get()
            .months
            .stand_alone
            .as_ref()
            .unwrap()
            .narrow
            .is_none());
        assert!(cs_dates
            .get()
            .months
            .stand_alone
            .as_ref()
            .unwrap()
            .wide
            .is_some());

        // Czech weekdays are unaliased because they completely overlap.
        assert!(cs_dates.get().weekdays.stand_alone.is_none());
    }
}
