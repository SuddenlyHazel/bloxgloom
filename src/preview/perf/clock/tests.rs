use super::*;

#[test]
fn default_clock_samples_realtime_while_fixed_clock_never_calls_it() {
    let default = WaterClock::parse(None).unwrap();
    assert_eq!(default.sample(|| 1.25), 1.25);
    assert_eq!(default.sample(|| 2.5), 2.5);
    for value in ["0", "19.75", "1e2"] {
        let clock = WaterClock::parse(Some(OsStr::new(value))).unwrap();
        let expected = value.parse::<f32>().unwrap();
        assert_eq!(
            clock.sample(|| panic!("fixed clock sampled realtime")),
            expected
        );
        assert_eq!(clock.sample(|| panic!("fixed clock drifted")), expected);
    }
}

#[test]
fn invalid_fixed_clock_fails_instead_of_silently_using_realtime() {
    for value in ["", "bad", "-0.1", "NaN", "inf", "-inf", "1e100"] {
        assert!(
            WaterClock::parse(Some(OsStr::new(value))).is_err(),
            "{value}"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert!(WaterClock::parse(Some(OsStr::from_bytes(&[0xff]))).is_err());
    }
}
