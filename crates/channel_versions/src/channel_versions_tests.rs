use super::*;

#[test]
fn test_parse_version_string() {
    let version_string = "v0.2023.05.15.08.04.stable_01";
    let parsed_version: ParsedVersion = version_string
        .try_into()
        .expect("version string is parsable");
    // major.year.month.day.hour.minute.patch
    assert_eq!(parsed_version.components, vec![0, 2023, 5, 15, 8, 4, 1]);
    assert_eq!(parsed_version.prerelease, None);
}

#[test]
fn test_official_version_rejects_impossible_date() {
    // The date half is still validated as a date, not as three more integers.
    assert!(ParsedVersion::try_from("v0.2023.13.45.08.04.stable_01").is_err());
}

#[test]
fn test_major_versions_compare_correctly() {
    let older_version: ParsedVersion = "v0.2023.05.15.08.04.stable_01"
        .try_into()
        .expect("older_version is parsable");
    let newer_version: ParsedVersion = "v1.2023.05.15.08.04.stable_01"
        .try_into()
        .expect("newer_version is parsable");
    assert!(newer_version > older_version);
}

#[test]
fn test_dates_compare_correctly() {
    let older_version: ParsedVersion = "v0.2023.05.15.08.04.stable_01"
        .try_into()
        .expect("older_version is parsable");
    let newer_version: ParsedVersion = "v0.2023.05.22.08.04.stable_00"
        .try_into()
        .expect("newer_version is parsable");
    assert!(newer_version > older_version);
}

#[test]
fn test_patches_compare_correctly() {
    let older_version: ParsedVersion = "v0.2023.05.15.08.04.stable_00"
        .try_into()
        .expect("older_version is parsable");
    let newer_version: ParsedVersion = "v0.2023.05.15.08.04.stable_01"
        .try_into()
        .expect("newer_version is parsable");
    assert!(newer_version > older_version);
}

#[test]
fn test_ignores_unknown_channels() {
    // We no longer support or parse-out beta and canary versions, but we
    // need to be able to parse a JSON file that still contains them.
    let channel_version_string = r#"{
        "beta": {
          "version": "v0.2024.01.30.16.52.beta_00"
        },
        "canary": {
          "version": "v0.2022.09.29.08.08.canary_00"
        },
        "dev": {
          "version": "v0.2024.01.30.20.34.dev_00"
        },
        "preview": {
          "version": "v0.2024.01.30.20.34.preview_00"
        },
        "stable": {
          "version": "v0.2024.01.16.16.31.stable_01"
        }
      }"#;

    let channel_versions: ChannelVersions = serde_json::from_str(channel_version_string)
        .expect("Should be able to parse channel versions");
    assert_eq!(
        channel_versions.stable.version_info().version,
        "v0.2024.01.16.16.31.stable_01"
    );
}

// The tags below are the shapes this repository's release pipeline actually
// emits -- taken from `git tag` and from `.github/workflows/phosphor_release.yml`
// -- not synthetic examples. The previous version of this section tested only
// `vYYYY.MM.DD.N`, a shape no release here has ever used, so it passed while
// every real tag failed to parse and every guard built on `ParsedVersion`
// silently went inert.
//
// Real tags, as of writing:
//   v0.1.0
//   v0.1.1
//   v2026.08.14.1-beta
//   v0.<date +%Y.%m.%d.%H%M>   (phosphor_release.yml, workflow_dispatch branch)

#[test]
fn test_repo_semver_tags_parse_and_order() {
    let older: ParsedVersion = "v0.1.0".try_into().expect("v0.1.0 should parse");
    let newer: ParsedVersion = "v0.1.1".try_into().expect("v0.1.1 should parse");
    assert_eq!(newer.components, vec![0, 1, 1]);
    assert_eq!(newer.prerelease, None);
    assert!(newer > older);
}

#[test]
fn test_repo_beta_tag_parses() {
    let parsed: ParsedVersion = "v2026.08.14.1-beta"
        .try_into()
        .expect("the published beta tag should parse");
    // The leading synthetic 0 puts a bare date tag on the same
    // major.YYYY.MM.DD axis as the dispatch-generated v0.YYYY.MM.DD.HHMM. The
    // fourth segment stays where it is: it is the build counter.
    assert_eq!(parsed.components, vec![0, 2026, 8, 14, 1]);
    assert_eq!(parsed.prerelease.as_deref(), Some("beta"));
}

#[test]
fn test_dispatch_generated_tag_parses() {
    // phosphor_release.yml: TAG="v0.$(date +%Y.%m.%d.%H%M)"
    let parsed: ParsedVersion = "v0.2026.08.21.1430"
        .try_into()
        .expect("the workflow_dispatch tag should parse");
    // HHMM is a clock reading, not a build counter, so it gets its own
    // hour/minute axis below the (absent, hence 0) counter. Leaving it fused
    // with the counter slot is what made a 09:30 dispatch build outrank the
    // same day's v2026.08.21.1 -- see the test below.
    assert_eq!(parsed.components, vec![0, 2026, 8, 21, 0, 14, 30]);
    assert_eq!(parsed.prerelease, None);
}

#[test]
fn test_dispatch_build_does_not_outrank_the_days_numbered_release() {
    // Both shapes come out of the same workflow file: run the workflow_dispatch
    // branch in the morning, cut the real release later the same day. If the
    // clock reading and the build counter share a slot, 930 > 1 and everyone on
    // the dispatch build declines the release for good.
    let dispatch: ParsedVersion = "v0.2026.08.21.0930".try_into().unwrap();
    let release: ParsedVersion = "v2026.08.21.1".try_into().unwrap();
    assert!(
        release > dispatch,
        "the numbered release must supersede the same day's dispatch build"
    );
    // ...while still ranking above the previous day's dispatch build, and above
    // an unnumbered tag of its own date.
    let yesterday: ParsedVersion = "v0.2026.08.20.2359".try_into().unwrap();
    let unnumbered: ParsedVersion = "v2026.08.21".try_into().unwrap();
    assert!(dispatch > yesterday);
    assert!(dispatch > unnumbered);
}

#[test]
fn test_four_digit_clock_is_not_confused_with_a_build_counter() {
    // `date +%H%M` zero-pads, so a dispatch tag's last segment is always four
    // digits. A hand-cut counter is not, and must keep its own slot.
    let counter: ParsedVersion = "v0.2026.08.21.3".try_into().unwrap();
    assert_eq!(counter.components, vec![0, 2026, 8, 21, 3]);
    let midnight: ParsedVersion = "v0.2026.08.21.0003".try_into().unwrap();
    assert_eq!(midnight.components, vec![0, 2026, 8, 21, 0, 0, 3]);
    assert!(counter > midnight);
}

#[test]
fn test_dispatch_generated_tags_order_by_timestamp() {
    let morning: ParsedVersion = "v0.2026.08.21.0930".try_into().unwrap();
    let afternoon: ParsedVersion = "v0.2026.08.21.1430".try_into().unwrap();
    let previous_day: ParsedVersion = "v0.2026.08.20.2359".try_into().unwrap();
    assert!(afternoon > morning);
    assert!(morning > previous_day);
}

#[test]
fn test_beta_is_not_downgraded_to_the_semver_release() {
    // The regression this whole module exists for. A user running the beta gets
    // `v0.1.1` back from `/releases/latest`, because betas are published with
    // `make_latest:false` and that endpoint skips prereleases. The downgrade
    // guard only works if the beta parses *and* compares greater.
    let installed: ParsedVersion = "v2026.08.14.1-beta"
        .try_into()
        .expect("installed beta tag must parse, or the guard is inert");
    // `github::GithubRelease::version()` has already trimmed the `v`.
    let offered: ParsedVersion = "0.1.1"
        .try_into()
        .expect("the latest-release version must parse");
    assert!(
        installed > offered,
        "the beta must not be treated as older than v0.1.1"
    );
}

#[test]
fn test_v_prefix_is_optional_and_does_not_change_ordering() {
    // `ChannelState::app_version()` keeps the `v`; `GithubRelease::version()`
    // trims it. Both sides must land on the same ParsedVersion.
    let prefixed: ParsedVersion = "v0.1.1".try_into().unwrap();
    let bare: ParsedVersion = "0.1.1".try_into().unwrap();
    assert_eq!(prefixed, bare);
}

#[test]
fn test_channel_labelled_tag_parses() {
    // `app/src/autoupdate/mod.rs` documents this shape for `GIT_RELEASE_TAG`,
    // and `mod_test.rs`'s openWarp cases compare two of them; the label can be
    // attached with a `.` as well as with a `-`.
    let older: ParsedVersion = "v2026.05.10.preview".try_into().unwrap();
    let newer: ParsedVersion = "2026.05.11.preview".try_into().unwrap();
    assert_eq!(older.components, vec![0, 2026, 5, 10]);
    assert_eq!(older.prerelease.as_deref(), Some("preview"));
    assert!(newer > older);
}

#[test]
fn test_prerelease_sorts_before_its_release() {
    let prerelease: ParsedVersion = "v2026.08.14.1-beta".try_into().unwrap();
    let release: ParsedVersion = "v2026.08.14.1".try_into().unwrap();
    assert!(release > prerelease);
}

#[test]
fn test_prerelease_identifiers_compare_as_semver() {
    // phosphor_release.yml documents `v2026.08.01-beta.1`, so the label is
    // dot-separated and its numeric identifiers have to compare as numbers. A
    // plain string compare put `beta.10` below `beta.2`, turning the tenth beta
    // of a date into a downgrade.
    let second: ParsedVersion = "v2026.08.01-beta.2".try_into().unwrap();
    let tenth: ParsedVersion = "v2026.08.01-beta.10".try_into().unwrap();
    assert!(tenth > second, "beta.10 must outrank beta.2");

    // "A larger set of pre-release fields has a higher precedence than a
    // smaller set, if all of the preceding identifiers are equal."
    let bare: ParsedVersion = "v2026.08.01-beta".try_into().unwrap();
    assert!(second > bare);

    // "Numeric identifiers always have lower precedence than alphanumeric
    // identifiers."
    let alpha: ParsedVersion = "v2026.08.01-beta.rc".try_into().unwrap();
    assert!(alpha > tenth);

    // A leading zero is not a canonical numeric identifier, so `beta.01` is
    // compared as text -- otherwise it would tie with `beta.1` while remaining
    // an unequal value, and `Ord` would contradict `PartialEq`.
    let padded: ParsedVersion = "v2026.08.01-beta.01".try_into().unwrap();
    let first: ParsedVersion = "v2026.08.01-beta.1".try_into().unwrap();
    assert_ne!(padded, first);
    assert_ne!(padded.cmp(&first), std::cmp::Ordering::Equal);
}

#[test]
fn test_prerelease_case_is_preserved() {
    // `-RC1` and `-rc1` are different tags; lower-casing the label made them
    // compare `Equal`, so an update between them read as "already current".
    let upper: ParsedVersion = "v2026.08.01-RC1".try_into().unwrap();
    let lower: ParsedVersion = "v2026.08.01-rc1".try_into().unwrap();
    assert_eq!(upper.prerelease.as_deref(), Some("RC1"));
    assert_ne!(upper, lower);
    assert_ne!(upper.cmp(&lower), std::cmp::Ordering::Equal);
}

#[test]
fn test_deserialization_cannot_bypass_the_trailing_zero_invariant() {
    // The fields are private because `new()` trims trailing zeros, which is
    // what keeps the zero-padding `Ord` consistent with the derived
    // `PartialEq`. A derived `Deserialize` would have gone around it.
    let deserialized: ParsedVersion =
        serde_json::from_str(r#"{"components": [0, 1, 0, 0], "prerelease": null}"#)
            .expect("a ParsedVersion should deserialize");
    let parsed: ParsedVersion = "v0.1".try_into().unwrap();
    assert_eq!(deserialized.components, vec![0, 1]);
    assert_eq!(deserialized, parsed);
    assert_eq!(deserialized.cmp(&parsed), std::cmp::Ordering::Equal);
}

/// The all-zero case, which the previous `len() > 1` trim guard left alone:
/// `[]` and `[0]` are `Equal` under the zero-padding `Ord`, so `PartialEq` has
/// to agree, and the only way it can is if both canonicalise to the same list.
/// The old test above only exercised `[0, 1, 0, 0]`, where the guard never
/// bites.
#[test]
fn test_deserialization_canonicalises_an_all_zero_version() {
    let empty: ParsedVersion = serde_json::from_str(r#"{"components": []}"#)
        .expect("a ParsedVersion should deserialize");
    let zero: ParsedVersion = serde_json::from_str(r#"{"components": [0]}"#)
        .expect("a ParsedVersion should deserialize");
    let many_zeros: ParsedVersion = serde_json::from_str(r#"{"components": [0, 0, 0, 0]}"#)
        .expect("a ParsedVersion should deserialize");

    assert!(empty.components.is_empty());
    assert_eq!(empty.components, zero.components);
    assert_eq!(empty.components, many_zeros.components);

    for (a, b) in [(&empty, &zero), (&empty, &many_zeros), (&zero, &many_zeros)] {
        assert_eq!(a, b, "PartialEq must agree with Ord");
        assert_eq!(a.cmp(b), std::cmp::Ordering::Equal);
    }

    // And the canonical empty list still orders correctly against real
    // versions rather than falling out of the total order.
    let real: ParsedVersion = "v0.1".try_into().unwrap();
    assert!(empty < real);
    assert!(real > zero);
}

#[test]
fn test_trailing_zero_components_do_not_break_equality() {
    // `Ord` zero-pads the shorter component list, so `PartialEq` has to agree.
    let short: ParsedVersion = "v0.1".try_into().unwrap();
    let padded: ParsedVersion = "v0.1.0".try_into().unwrap();
    assert_eq!(short, padded);
    assert_eq!(short.cmp(&padded), std::cmp::Ordering::Equal);
}

#[test]
fn test_rollback_of_a_date_tag_is_detected() {
    // A rolled-back release must not read as an upgrade.
    let installed: ParsedVersion = "v0.2026.08.21.1430".try_into().unwrap();
    let rolled_back: ParsedVersion = "v0.2026.08.20.0900".try_into().unwrap();
    assert!(installed > rolled_back);
}

#[test]
fn test_non_version_strings_are_rejected() {
    // `warp_tui::autoupdate::parse_safe_version_component` uses a successful
    // parse as half of its path-traversal gate, so these must keep failing.
    for invalid in [
        "",
        ".",
        "..",
        "v1..dev",
        "../v1",
        "nested/v1",
        "version:stream",
        "CON",
        "trailing.",
        "contains space",
        "preview-1",
        "A",
        // The numeric group used to hand segments back to the label, so all of
        // these parsed: `v0.1.1-` as `[0, 1]` + label `1-`, `v1.2.3.` as
        // `[1, 2]` + label `3.`, and the nine-segment string as eight
        // components plus a label of `9`.
        "v0.1.1-",
        "v1.2.3.",
        "v0.1.1.",
        "v1.2.3.4.5.6.7.8.9",
        "v1.2.3-beta.",
        "v1.2.3-beta..1",
    ] {
        assert!(
            ParsedVersion::try_from(invalid).is_err(),
            "{invalid:?} should not parse as a version"
        );
    }
}
