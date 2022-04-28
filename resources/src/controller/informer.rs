use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc;

use crate::controller::{
    reflector::{Reflector, ReflectorNotification},
    ListerWatcher, Store,
};

pub struct Informer {
    store: Arc<Store>,
    reflector: Arc<Reflector>,
}

impl Informer {
    pub(super) fn new(lw: ListerWatcher) -> Self {
        let store = Arc::new(DashMap::new());
        let reflector = Reflector {
            lw,
            store: store.clone(),
        };
        Self {
            store,
            reflector: Arc::new(reflector),
        }
    }

    pub(super) async fn run(&self) -> anyhow::Result<()> {
        // start reflector
        let (tx, mut rx) = mpsc::channel::<ReflectorNotification>(16);
        let r = self.reflector.clone();
        let reflector_handle = tokio::spawn(async move { r.run(tx).await });

        while let Some(n) = rx.recv().await {
            println!("{:#?}", n);
        }

        reflector_handle.await?
    }
}
