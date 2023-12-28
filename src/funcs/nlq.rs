use anyhow::{bail, format_err, Error};
use dolby_vision::rpu::dovi_rpu::DoviRpu;
use itertools::Itertools;

use vapoursynth::core::CoreRef;
use vapoursynth::plugins::*;
use vapoursynth::prelude::*;
use vapoursynth::video_info::VideoInfo;

pub struct MapNLQ<'core> {
    pub bl: Node<'core>,
    pub el: Node<'core>,
    pub rpus: Option<Vec<DoviRpu>>,
}

impl<'core> Filter<'core> for MapNLQ<'core> {
    fn video_info(&self, _api: API, _core: CoreRef<'core>) -> Vec<VideoInfo<'core>> {
        let info = self.bl.info();
        let format = match info.format {
            Property::Variable => unreachable!(),
            Property::Constant(format) => format,
        };

        // Output the same format as source
        vec![VideoInfo {
            format: Property::Constant(
                _core
                    .register_format(
                        ColorFamily::YUV,
                        format.sample_type(),
                        12,
                        format.sub_sampling_w(),
                        format.sub_sampling_h(),
                    )
                    .unwrap(),
            ),
            flags: info.flags,
            framerate: info.framerate,
            num_frames: info.num_frames,
            resolution: info.resolution,
        }]
    }

    fn get_frame_initial(
        &self,
        _api: API,
        _core: CoreRef<'core>,
        context: FrameContext,
        n: usize,
    ) -> Result<Option<FrameRef<'core>>, Error> {
        self.bl.request_frame_filter(context, n);
        self.el.request_frame_filter(context, n);
        Ok(None)
    }

    fn get_frame(
        &self,
        _api: API,
        core: CoreRef<'core>,
        context: FrameContext,
        n: usize,
    ) -> Result<FrameRef<'core>, Error> {
        let bl_frame = self
            .bl
            .get_frame_filter(context, n)
            .ok_or_else(|| format_err!("Couldn't get the BL frame"))?;

        let el_frame = self
            .el
            .get_frame_filter(context, n)
            .ok_or_else(|| format_err!("Couldn't get the EL frame"))?;

        // From RPU list file
        let mut existing_rpu = if let Some(rpus) = &self.rpus {
            assert!(n < rpus.len());
            Some(&rpus[n])
        } else {
            None
        };

        // From frame props if available
        let parsed_rpu = if existing_rpu.is_none() {
            let props = el_frame.props();
            let rpu_data = props.get_data("DolbyVisionRPU")?;

            Some(DoviRpu::parse_unspec62_nalu(rpu_data).unwrap())
        } else {
            None
        };

        if let Some(rpu) = &parsed_rpu {
            existing_rpu.replace(rpu);
        }

        let rpu = existing_rpu.unwrap();
        let mapping = rpu.rpu_data_mapping.as_ref().unwrap();
        let num_pivots = (mapping.nlq_num_pivots_minus2.unwrap() + 1) as usize;
        assert!(num_pivots == 1);

        let rpu_data_nlq = mapping.nlq.as_ref().unwrap();

        assert!(rpu.dovi_profile == 7);

        let format = el_frame.format();

        if format.sample_type() == SampleType::Float {
            bail!("Floating point formats are not supported");
        }

        let depth = el_frame.format().bits_per_sample();

        assert_eq!(el_frame.format().sample_type(), SampleType::Integer);

        let out_bit_depth = rpu.header.vdr_bit_depth_minus8 + 8;
        let el_bit_depth = rpu.header.el_bit_depth_minus8 + 8;
        let coeff_log2_denom = rpu.header.coefficient_log2_denom;
        let disable_residual_flag = rpu.header.disable_residual_flag;

        let resolution = bl_frame.resolution(0);

        let new_format = core
            .register_format(
                ColorFamily::YUV,
                format.sample_type(),
                12,
                format.sub_sampling_w(),
                format.sub_sampling_h(),
            )
            .unwrap();

        let mut new_frame = unsafe {
            FrameRefMut::new_uninitialized(core, Some(&bl_frame), new_format, resolution)
        };

        if !new_frame.props().keys().contains(&"DolbyVisionRPU") {
            new_frame.props_mut().set_data(
                "DolbyVisionRPU",
                el_frame.props().get_data("DolbyVisionRPU")?,
            )?;
        }

        let nlq_offsets = rpu_data_nlq.nlq_offset;
        let hdr_in_max_int = rpu_data_nlq.vdr_in_max_int;
        let hdr_in_max = rpu_data_nlq.vdr_in_max;

        let linear_deadzone_slope_int = rpu_data_nlq.linear_deadzone_slope_int;
        let linear_deadzone_slope = rpu_data_nlq.linear_deadzone_slope;

        let linear_deadzone_threshold_int = rpu_data_nlq.linear_deadzone_threshold_int;
        let linear_deadzone_threshold = rpu_data_nlq.linear_deadzone_threshold;

        let mut fp_hdr_in_max = [0_i64; 3];
        let mut fp_linear_deadzone_slope = [0_i64; 3];
        let mut fp_linear_deadzone_threshold = [0_i64; 3];

        for cmp in 0..3_usize {
            fp_hdr_in_max[cmp] =
                ((hdr_in_max_int[cmp] << coeff_log2_denom) as i64) + (hdr_in_max[cmp] as i64);
            fp_linear_deadzone_slope[cmp] = ((linear_deadzone_slope_int[cmp] << coeff_log2_denom)
                as i64)
                + (linear_deadzone_slope[cmp] as i64);
            fp_linear_deadzone_threshold[cmp] = ((linear_deadzone_threshold_int[cmp]
                << coeff_log2_denom) as i64)
                + (linear_deadzone_threshold[cmp] as i64);
        }

        let maxout = (1 << out_bit_depth) - 1;

        match depth {
            0..=8 => unreachable!(),
            9..=16 => {
                for cmp in 0..3_usize {
                    let bl_plane = bl_frame.plane::<u16>(cmp)?;
                    let el_plane = el_frame.plane::<u16>(cmp)?;
                    let out_plane = new_frame.plane_mut::<u16>(cmp)?;

                    let thresh = fp_linear_deadzone_threshold[cmp];
                    let slope = fp_linear_deadzone_slope[cmp];
                    let fp_in_max = fp_hdr_in_max[cmp];

                    bl_plane
                        .iter()
                        .zip(el_plane.iter())
                        .zip(out_plane.iter_mut())
                        .for_each(|((bl_pixel, el_pixel), out_pixel)| {
                            let mut tmp = (*el_pixel as i64) - (nlq_offsets[cmp] as i64);

                            let result = if tmp == 0 {
                                0
                            } else {
                                let sign = if tmp < 0 { -1 } else { 1 };

                                tmp <<= 1;
                                tmp -= sign;
                                tmp <<= 10 - el_bit_depth;

                                let mut dq = tmp * slope;
                                let tt = (thresh << (10 - el_bit_depth + 1)) * sign;
                                dq += tt;

                                let rr = fp_in_max << (10 - el_bit_depth + 1);

                                dq = dq.clamp(-rr, rr);

                                dq >> (coeff_log2_denom - 5 - el_bit_depth)
                            };

                            let mut h = *bl_pixel as i64;

                            if !disable_residual_flag {
                                h += result;
                            }

                            h += 1 << (15 - out_bit_depth);
                            h >>= 16 - out_bit_depth;

                            h = h.clamp(0, maxout);

                            *out_pixel = h as u16;
                        });
                }
            }
            17..=32 => unreachable!(),
            _ => unreachable!(),
        }

        Ok(new_frame.into())
    }
}
