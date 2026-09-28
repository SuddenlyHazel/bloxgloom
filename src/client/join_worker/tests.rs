use super::*;

pub(in crate::client) fn abandoned_result_retires_transport(address: &str) {
    completed_result_can_still_be_cancelled(address);
    let address = address.to_owned();
    let (connected, ready) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let mut attempt = Attempt::spawn(move |control| {
        let mut network = Network::connect_controlled(&address, 1, 0x11fec9, control)?;
        let stopped = network.take_worker_completion().unwrap();
        connected
            .send((stopped, Arc::downgrade(&network.catalog)))
            .unwrap();
        released.recv_timeout(Duration::from_secs(10)).unwrap();
        Ok(Prepared {
            network,
            config: Config::default(),
        })
    })
    .unwrap();
    let (stopped, catalog) = ready.recv_timeout(Duration::from_secs(10)).unwrap();
    let join_stopped = attempt.stopped.take().unwrap();
    // The window can close after preparation but before delivery. A completed
    // result must not keep the session alive in an abandoned result mailbox.
    drop(attempt);
    release.send(()).unwrap();
    join_stopped
        .recv_timeout(Duration::from_secs(10))
        .expect("abandoned join worker retained");
    for _ in 0..2 {
        stopped
            .recv_timeout(Duration::from_secs(10))
            .expect("abandoned transport worker retained");
    }
    assert!(catalog.upgrade().is_none());
}

fn completed_result_can_still_be_cancelled(address: &str) {
    let address = address.to_owned();
    let (connected, ready) = mpsc::sync_channel(1);
    let mut attempt = Attempt::spawn(move |control| {
        let mut network = Network::connect_controlled(&address, 1, 0x11feca, control)?;
        connected
            .send((
                network.take_worker_completion().unwrap(),
                Arc::downgrade(&network.catalog),
            ))
            .unwrap();
        Ok(Prepared {
            network,
            config: Config::default(),
        })
    })
    .unwrap();
    attempt.wait_finished_for_test();
    let (stopped, catalog) = ready.recv_timeout(Duration::from_secs(10)).unwrap();
    // Deadline expiry wins even if a successful result is already waiting.
    // No worker-completion race may resurrect a cancelled candidate session.
    attempt.started = Instant::now() - Duration::from_secs(61);
    let error = attempt
        .poll()
        .unwrap()
        .err()
        .expect("expired result admitted");
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    for _ in 0..2 {
        stopped
            .recv_timeout(Duration::from_secs(10))
            .expect("cancelled ready transport retained");
    }
    assert!(catalog.upgrade().is_none());
}
