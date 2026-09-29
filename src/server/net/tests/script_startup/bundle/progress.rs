//! Production transfer through a real listener, including cache and failed admission.
use super::*;

#[test]
fn received_byte_progress_is_contiguous_and_cache_reuse_transfers_nothing() {
    let fixture = fixture();
    gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
        let (mut peer, identity) = offer(address, 0x501);
        let mut progress = Vec::new();
        let cached =
            crate::client::bundle::receive_progress(&mut peer, identity, None, |bytes, cached| {
                progress.push((bytes, cached));
                Ok(())
            })
            .unwrap();
        welcome(&mut peer);
        drop(peer);
        assert_eq!(progress.first(), Some(&(0, false)));
        assert_eq!(progress.last(), Some(&(identity.total_len, false)));
        assert!(progress.len() >= 4);
        assert!(
            progress.windows(2).all(
                |pair| pair[1].0 > pair[0].0 && pair[1].0 - pair[0].0 <= MAX_BUNDLE_PART as u32
            )
        );
        let (mut peer, identity) = offer(address, 0x502);
        progress.clear();
        let reused = crate::client::bundle::receive_progress(
            &mut peer,
            identity,
            Some(cached.clone()),
            |bytes, cached| {
                progress.push((bytes, cached));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(progress, [(0, true)]);
        assert!(Arc::ptr_eq(&cached, &reused));
        welcome(&mut peer);
        drop(peer);

        // Compatibility must be checked even for an exact verified cache hit.
        let (mut peer, identity) = offer(address, 0x503);
        let unsupported = BundleIdentity {
            client_runtime: identity.client_runtime + 1,
            ..identity
        };
        let error =
            crate::client::bundle::receive(&mut peer, unsupported, Some(cached)).unwrap_err();
        assert!(error.to_string().contains("client runtime contract"));
        peer.shutdown(Shutdown::Both).unwrap();
        drop(peer);
        let (mut healthy, identity) = offer(address, 0x504);
        crate::client::bundle::receive(&mut healthy, identity, None).unwrap();
        welcome(&mut healthy);
    });
}

#[test]
fn cancelled_progress_callback_never_acknowledges_bundle_readiness() {
    let fixture = fixture();
    gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
        let (mut peer, identity) = offer(address, 0x505);
        let error =
            crate::client::bundle::receive_progress(&mut peer, identity, None, |bytes, _| {
                if bytes > 0 {
                    Err(io::Error::new(ErrorKind::Interrupted, "cancelled"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Interrupted);
        peer.shutdown(Shutdown::Both).unwrap();
        drop(peer);
        let (mut healthy, identity) = offer(address, 0x506);
        crate::client::bundle::receive(&mut healthy, identity, None).unwrap();
        welcome(&mut healthy);
    });
}
