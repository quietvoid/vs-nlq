#[macro_use]
extern crate vapoursynth;

#[macro_use]
extern crate failure;

use std::path::Path;

use vapoursynth::core::CoreRef;
use vapoursynth::plugins::*;
use vapoursynth::prelude::*;

use failure::Error;

mod funcs;
use funcs::*;

const PLUGIN_IDENTIFIER: &str = "com.quietvoid";

make_filter_function! {
    MapNLQFunction, "MapNLQ"

    fn create_dovi<'core>(
        _api: API,
        _core: CoreRef<'core>,
        bl: Node<'core>,
        el: Node<'core>,
        rpu: Option<&[u8]>,
    ) -> Result<Option<Box<dyn Filter<'core> + 'core>>, Error> {
        let rpus = if let Some(path) = rpu {
            let rpu_path = Path::new(std::str::from_utf8(path)?);

            let res = parse_rpu_file(rpu_path);
            
            if let Ok(rpus) = res {
                rpus
            } else {
                None
            }
        } else {
            None
        };

        Ok(Some(Box::new(MapNLQ { bl, el, rpus })))
    }
}

export_vapoursynth_plugin! {
    Metadata {
        identifier: PLUGIN_IDENTIFIER,
        namespace: "vsnlq",
        name: "test",
        read_only: false,
    },
    [
        MapNLQFunction::new(),
    ]
}
