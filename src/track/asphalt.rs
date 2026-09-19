//! The asphalt image uses world-space UVs, so corners and the lap join tile cleanly.
use bevy::{
    image::{
        ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler,
        ImageSamplerDescriptor,
    },
    prelude::*,
    render::render_resource::TextureFormat,
};

#[derive(Resource)]
pub(super) struct Texture {
    image: Handle<Image>,
    ready: bool,
}

pub(super) fn material(commands: &mut Commands, assets: &AssetServer) -> StandardMaterial {
    let image = assets
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                anisotropy_clamp: 16,
                ..default()
            });
        })
        .load("textures/racing-asphalt.png");
    commands.insert_resource(Texture {
        image: image.clone(),
        ready: false,
    });
    StandardMaterial {
        base_color: Color::srgb(0.82, 0.82, 0.82),
        base_color_texture: Some(image),
        perceptual_roughness: 0.96,
        reflectance: 0.12,
        ..default()
    }
}

// PNGs arrive with one mip level. Build the smaller levels in linear light,
// preserving average brightness and avoiding sparkling aggregate at distance.
pub(super) fn prepare(
    mut texture: Option<ResMut<Texture>>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    let (Some(texture), Some(images)) = (texture.as_mut(), images.as_mut()) else {
        return;
    };
    if texture.ready {
        return;
    }
    let Some(mut image) = images.get_mut(&texture.image) else {
        return;
    };
    mipmaps(&mut image);
    texture.ready = true;
}

fn mipmaps(image: &mut Image) {
    assert_eq!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8UnormSrgb
    );
    let Some(data) = image.data.as_mut() else {
        return;
    };
    let mut width = image.texture_descriptor.size.width as usize;
    let mut height = image.texture_descriptor.size.height as usize;
    let mut previous = data.clone();
    let mut levels = 1;
    let linear: [f32; 256] = std::array::from_fn(|i| {
        let c = i as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    while width > 1 || height > 1 {
        let next_width = (width / 2).max(1);
        let next_height = (height / 2).max(1);
        let mut next = Vec::with_capacity(next_width * next_height * 4);
        for y in 0..next_height {
            for x in 0..next_width {
                for channel in 0..3 {
                    let mut sum = 0.0;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let at = (((y * 2 + dy).min(height - 1) * width
                                + (x * 2 + dx).min(width - 1))
                                * 4)
                                + channel;
                            sum += linear[previous[at] as usize];
                        }
                    }
                    let c = sum * 0.25;
                    let srgb = if c <= 0.0031308 {
                        c * 12.92
                    } else {
                        1.055 * c.powf(1.0 / 2.4) - 0.055
                    };
                    next.push((srgb * 255.0).round().clamp(0.0, 255.0) as u8);
                }
                next.push(255);
            }
        }
        data.extend_from_slice(&next);
        previous = next;
        width = next_width;
        height = next_height;
        levels += 1;
    }
    image.texture_descriptor.mip_level_count = levels;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::RenderAssetUsages,
        render::render_resource::{Extent3d, TextureDimension},
    };

    #[test]
    fn mip_levels_preserve_colour_and_cover_every_size() {
        let mut image = Image::new_fill(
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[80, 90, 100, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        mipmaps(&mut image);
        assert_eq!(image.texture_descriptor.mip_level_count, 4);
        let data = image.data.unwrap();
        assert_eq!(data.len(), (64 + 16 + 4 + 1) * 4);
        assert!(
            data.chunks_exact(4)
                .all(|pixel| pixel == [80, 90, 100, 255])
        );
    }
}
