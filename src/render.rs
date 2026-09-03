mod commands;
mod core;
mod docked;
mod equalizer;
mod main;
mod playlist;
mod surface;

pub use crate::skin::layout::{
    equalizer_control_spec, equalizer_slider_layout, equalizer_window_height,
    main_push_button_spec, main_slider_layout, main_toggle_button_spec, main_window_height,
    playlist_window_height, EqualizerControl, EqualizerSlider, MainPushButton, MainSlider,
    MainToggleButton, SkinRect, SliderLayout, SpriteSpec, EQUALIZER_WINDOW_HEIGHT,
    EQUALIZER_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
    PLAYLIST_DEFAULT_HEIGHT, PLAYLIST_DEFAULT_WIDTH, PLAYLIST_HEIGHT_BASE, PLAYLIST_MIN_HEIGHT,
    PLAYLIST_MIN_WIDTH, PLAYLIST_WIDTH_STEP,
};
pub use commands::*;
pub use core::*;
pub use docked::*;
pub use equalizer::*;
pub use main::*;
pub use playlist::*;
pub use surface::*;

#[cfg(test)]
mod tests {
    use super::*;
    use super::{Context, Format, ImageSurface};

    use crate::skin::xpm::XpmImage;
    use crate::skin::{DefaultSkin, SkinPixmapKind};

    #[test]
    fn creates_image_surface_from_xpm_pixels() {
        let image = XpmImage::parse(
            r#"/* XPM */
            static char *x[] = {
            "2 1 2 1",
            "a c #000000",
            "b c None",
            "ab"
            };"#,
        )
        .unwrap();

        let surface = surface_from_xpm(&image).unwrap();
        assert_eq!(surface.width(), 2);
        assert_eq!(surface.height(), 1);

        let data = surface.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(data[0..4].try_into().unwrap()),
            0xff00_0000
        );
        assert_eq!(u32::from_ne_bytes(data[4..8].try_into().unwrap()), 0);
        assert!(surface.to_tiny_skia_pixmap().is_some());
    }

    #[test]
    fn caches_xpm_surfaces_until_image_pixels_change() {
        let mut image = XpmImage::from_argb_pixels(2, 1, vec![0xff01_0203, 0xff04_0506]).unwrap();

        let first = surface_from_xpm(&image).unwrap();
        let second = surface_from_xpm(&image).unwrap();
        assert!(first.shares_storage_with(&second));

        image.set_pixel_rgba(0, 0, [10, 20, 30, 255]);
        let changed = surface_from_xpm(&image).unwrap();
        assert!(!first.shares_storage_with(&changed));
        assert_eq!(changed.pixel_argb(0, 0), 0xff0a_141e);
    }

    #[test]
    fn cloned_xpm_images_have_independent_surface_cache_entries() {
        let image = XpmImage::from_argb_pixels(1, 1, vec![0xff01_0203]).unwrap();
        let cloned = image.clone();
        assert_eq!(image, cloned);

        let original_surface = surface_from_xpm(&image).unwrap();
        let cloned_surface = surface_from_xpm(&cloned).unwrap();
        assert!(!original_surface.shares_storage_with(&cloned_surface));
    }

    #[test]
    fn blits_source_rect_to_destination() {
        let image = XpmImage::parse(
            r#"/* XPM */
            static char *x[] = {
            "2 1 2 1",
            "a c #010203",
            "b c #040506",
            "ab"
            };"#,
        )
        .unwrap();
        let source = surface_from_xpm(&image).unwrap();
        let dest = ImageSurface::create(Format::ARgb32, 2, 1).unwrap();
        let cr = Context::new(&dest).unwrap();

        assert!(blit_surface_rect(&cr, &source, SkinRect::new(1, 0, 1, 1), (0, 0)).unwrap());
        drop(cr);
        dest.flush();

        let data = dest.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(data[0..4].try_into().unwrap()),
            0xff04_0506
        );
        assert_eq!(u32::from_ne_bytes(data[4..8].try_into().unwrap()), 0);
    }

    #[test]
    fn blit_clips_negative_source_coordinates_like_c() {
        let image = XpmImage::parse(
            r#"/* XPM */
            static char *x[] = {
            "1 1 1 1",
            "a c #010203",
            "a"
            };"#,
        )
        .unwrap();
        let source = surface_from_xpm(&image).unwrap();
        let dest = ImageSurface::create(Format::ARgb32, 2, 1).unwrap();
        let cr = Context::new(&dest).unwrap();

        assert!(blit_surface_rect(&cr, &source, SkinRect::new(-1, 0, 2, 1), (0, 0)).unwrap());
        drop(cr);
        dest.flush();

        let data = dest.data().unwrap();
        assert_eq!(u32::from_ne_bytes(data[0..4].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_ne_bytes(data[4..8].try_into().unwrap()),
            0xff01_0203
        );
    }

    #[test]
    fn render_scaled_has_no_seams_at_fractional_scales() {
        // A texture made of two adjacent slices sampling the same source. At
        // fractional scales, scaling per slice leaves seams; render_scaled
        // composites at 1x and scales the finished image once, so the result is
        // identical to scaling the whole image a single time and never seams.
        let image = XpmImage::parse(
            r#"/* XPM */
            static char *x[] = {
            "10 4 2 1",
            "a c #ff0000",
            "b c #00ff00",
            "ababababab",
            "ababababab",
            "ababababab",
            "ababababab"
            };"#,
        )
        .unwrap();
        let source = surface_from_xpm(&image).unwrap();
        let base_w = 10;
        let base_h = 4;
        let render = |cr: &Context| -> Result<(), RenderError> {
            let mut x = 0;
            while x < base_w {
                blit_surface_rect(cr, &source, SkinRect::new(x, 0, 2, base_h), (x, 0))?;
                x += 2;
            }
            Ok(())
        };

        for dev_w in 11..=80 {
            let dev_h = (base_h as f64 * dev_w as f64 / base_w as f64).round() as i32;

            let scaled = ImageSurface::create(Format::ARgb32, dev_w, dev_h).unwrap();
            let cr = Context::new(&scaled).unwrap();
            render_scaled(&cr, dev_w, dev_h, base_w, base_h, |c, pass| {
                if pass.is_bitmap() {
                    render(c)
                } else {
                    Ok(())
                }
            })
            .unwrap();
            drop(cr);
            scaled.flush();

            // Reference: composite once at 1x, then scale the whole image once.
            let base = ImageSurface::create(Format::ARgb32, base_w, base_h).unwrap();
            let cr = Context::new(&base).unwrap();
            render(&cr).unwrap();
            drop(cr);
            base.flush();
            let reference = ImageSurface::create(Format::ARgb32, dev_w, dev_h).unwrap();
            let cr = Context::new(&reference).unwrap();
            cr.scale(
                f64::from(dev_w) / f64::from(base_w),
                f64::from(dev_h) / f64::from(base_h),
            );
            cr.set_source_surface(&base, 0.0, 0.0).unwrap();
            cr.source().set_filter(Filter::Nearest);
            cr.paint().unwrap();
            drop(cr);
            reference.flush();

            let stride = scaled.stride() as usize;
            let a = scaled.data().unwrap();
            let b = reference.data().unwrap();
            for y in 0..dev_h as usize {
                for x in 0..dev_w as usize {
                    let off = y * stride + x * 4;
                    let pa = u32::from_ne_bytes(a[off..off + 4].try_into().unwrap());
                    // Every pixel is fully opaque: no seam shows the background.
                    assert_eq!(
                        pa >> 24,
                        0xff,
                        "transparent seam at dev_w={dev_w} x={x} y={y}: {pa:08x}"
                    );
                    // And it matches scaling the whole image exactly once.
                    let pb = u32::from_ne_bytes(b[off..off + 4].try_into().unwrap());
                    assert_eq!(
                        pa, pb,
                        "diverges from single-scale at dev_w={dev_w} x={x} y={y}"
                    );
                }
            }
        }
    }

    #[test]
    fn scaling_helpers_match_c_rounding_and_clamping() {
        assert_eq!(clamp_scale_factor(0.5), 1.0);
        assert_eq!(clamp_scale_factor(6.0), 5.0);
        assert_eq!(scale_coord(11, 1.5), 17);
        assert_eq!(scale_dim(0, 1.0), 1);
    }

    #[test]
    fn applies_window_scale_from_actual_to_base_size() {
        let surface = ImageSurface::create(Format::ARgb32, 20, 20).unwrap();
        let cr = Context::new(&surface).unwrap();

        assert!(apply_window_scale(&cr, 20, 10, 10, 5));
        let matrix = cr.matrix();
        assert_eq!(matrix.xx(), 2.0);
        assert_eq!(matrix.yy(), 2.0);
        assert!(!apply_window_scale(&cr, 0, 10, 10, 5));
    }

    #[test]
    fn scaled_text_rasterizes_dense_device_pixels() {
        let surface = ImageSurface::create(Format::ARgb32, 64, 64).unwrap();
        let cr = Context::new(&surface).unwrap();
        cr.scale(2.0, 2.0);
        cr.set_source_rgb(0.0, 1.0, 0.0);
        cr.select_font_face("Helvetica", FontSlant::Normal, FontWeight::Bold);
        cr.set_font_size(10.0);
        cr.move_to(2.0, 12.0);
        cr.show_text("A").unwrap();
        drop(cr);
        surface.flush();

        let data = surface.data().unwrap();
        let stride = surface.stride() as usize;
        let mut has_horizontal_neighbor = false;
        let mut has_vertical_neighbor = false;
        for y in 0..surface.height() as usize - 1 {
            for x in 0..surface.width() as usize - 1 {
                let alpha = data[y * stride + x * 4 + 3];
                if alpha == 0 {
                    continue;
                }
                if data[y * stride + (x + 1) * 4 + 3] != 0 {
                    has_horizontal_neighbor = true;
                }
                if data[(y + 1) * stride + x * 4 + 3] != 0 {
                    has_vertical_neighbor = true;
                }
            }
        }
        assert!(
            has_horizontal_neighbor,
            "scaled text should not be sparse horizontally"
        );
        assert!(
            has_vertical_neighbor,
            "scaled text should not be sparse vertically"
        );
    }

    #[test]
    fn renders_main_titlebar_focused_and_unfocused_rows() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let titlebar = surface_from_xpm(skin.get(SkinPixmapKind::Titlebar).unwrap()).unwrap();

        let focused_dest =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT).unwrap();
        let focused_cr = Context::new(&focused_dest).unwrap();
        assert!(render_main_titlebar(&focused_cr, &skin, true, false).unwrap());
        drop(focused_cr);
        focused_dest.flush();

        let unfocused_dest =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT).unwrap();
        let unfocused_cr = Context::new(&unfocused_dest).unwrap();
        assert!(render_main_titlebar(&unfocused_cr, &skin, false, false).unwrap());
        drop(unfocused_cr);
        unfocused_dest.flush();

        let titlebar_stride = titlebar.stride() as usize;
        let titlebar_data = titlebar.data().unwrap();
        let focused_source_offset = 27 * 4;
        let unfocused_source_offset = titlebar_stride * 15 + 27 * 4;
        let expected_focused = u32::from_ne_bytes(
            titlebar_data[focused_source_offset..focused_source_offset + 4]
                .try_into()
                .unwrap(),
        );
        let expected_unfocused = u32::from_ne_bytes(
            titlebar_data[unfocused_source_offset..unfocused_source_offset + 4]
                .try_into()
                .unwrap(),
        );

        let focused_data = focused_dest.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(focused_data[0..4].try_into().unwrap()),
            expected_focused
        );
        drop(focused_data);

        let unfocused_data = unfocused_dest.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(unfocused_data[0..4].try_into().unwrap()),
            expected_unfocused
        );
    }

    #[test]
    fn windowshade_segmented_meter_matches_xmms_logical_pixel_layout() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let surface = ImageSurface::create(Format::ARgb32, 38, 5).unwrap();
        let cr = Context::new(&surface).unwrap();
        let mut state = VisualizationRenderState::default();
        state.data[0] = 1.0;
        render_windowshade_visualization(&cr, &skin, 0, 0, &state).unwrap();
        drop(cr);
        surface.flush();

        let argb = |rgb: [u8; 3]| {
            0xff00_0000 | (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])
        };
        let data = surface.data().unwrap();
        let stride = surface.stride() as usize;
        let pixel = |x: usize, y: usize| {
            let offset = y * stride + x * 4;
            u32::from_ne_bytes(data[offset..offset + 4].try_into().unwrap())
        };
        let colors = skin.vis_colors();

        assert_eq!(pixel(0, 0), argb(colors[17]));
        assert_eq!(pixel(15, 0), argb(colors[12]));
        assert_eq!(pixel(30, 0), argb(colors[2]));
        assert_eq!(pixel(3, 0), argb(colors[0]));
        assert_eq!(pixel(0, 2), argb(colors[0]));
        assert_eq!(pixel(35, 0), argb(colors[0]));
        assert_eq!(pixel(37, 4), argb(colors[0]));
    }

    #[test]
    fn renders_normal_and_windowshade_main_player_backgrounds() {
        let skin = DefaultSkin::load_bundled().unwrap();

        let normal =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT).unwrap();
        let normal_cr = Context::new(&normal).unwrap();
        assert!(render_main_player(&normal_cr, &skin, true, false).unwrap());
        drop(normal_cr);
        normal.flush();

        let shaded =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT).unwrap();
        let shaded_cr = Context::new(&shaded).unwrap();
        assert!(render_main_player(&shaded_cr, &skin, true, true).unwrap());
        drop(shaded_cr);
        shaded.flush();

        assert_eq!(main_window_height(false), MAIN_WINDOW_HEIGHT);
        assert_eq!(main_window_height(true), MAIN_TITLEBAR_HEIGHT);

        let normal_data = normal.data().unwrap();
        let shaded_data = shaded.data().unwrap();
        assert_ne!(u32::from_ne_bytes(normal_data[0..4].try_into().unwrap()), 0);
        assert_ne!(u32::from_ne_bytes(shaded_data[0..4].try_into().unwrap()), 0);
    }

    #[test]
    fn computes_and_renders_docked_panel_composition() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let state = DockedPanelState {
            equalizer_visible: true,
            playlist_visible: true,
            ..DockedPanelState::default()
        };
        let (width, height) = docked_panel_size(state);
        assert_eq!(width, MAIN_WINDOW_WIDTH);
        assert_eq!(
            height,
            MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT + PLAYLIST_DEFAULT_HEIGHT
        );

        let surface = ImageSurface::create(Format::ARgb32, width, height).unwrap();
        let cr = Context::new(&surface).unwrap();
        assert!(render_docked_panels(&cr, &skin, state).unwrap());
        drop(cr);
        surface.flush();

        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let main_pixel = u32::from_ne_bytes(data[0..4].try_into().unwrap());
        let playlist_offset = stride * (MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT) as usize;
        let playlist_pixel = u32::from_ne_bytes(
            data[playlist_offset..playlist_offset + 4]
                .try_into()
                .unwrap(),
        );
        assert_ne!(main_pixel, 0);
        assert_ne!(playlist_pixel, 0);
    }

    #[test]
    fn docked_panel_size_ignores_detached_panels() {
        let state = DockedPanelState {
            equalizer_visible: true,
            equalizer_detached: true,
            playlist_visible: true,
            playlist_detached: true,
            ..DockedPanelState::default()
        };

        assert_eq!(
            docked_panel_size(state),
            (MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT)
        );
    }
}
