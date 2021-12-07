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
    DOVIFunction, "DOVI"

    fn create_dovi<'core>(
        _api: API,
        _core: CoreRef<'core>,
        bl: Node<'core>,
        el: Node<'core>,
        rpu: &[u8],
    ) -> Result<Option<Box<dyn Filter<'core> + 'core>>, Error> {
        let rpu_path = Path::new(std::str::from_utf8(rpu)?);
        let rpus = parse_rpu_file(rpu_path).unwrap().unwrap();

        Ok(Some(Box::new(DOVIMap { bl, el, rpus })))
    }
}

export_vapoursynth_plugin! {
    Metadata {
        identifier: PLUGIN_IDENTIFIER,
        namespace: "dovi",
        name: "test",
        read_only: false,
    },
    [
        DOVIFunction::new(),
    ]
}
