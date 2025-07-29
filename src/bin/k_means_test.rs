use anyhow::Context;
use gen_rp_rs::k_means::dist_sq;
use image::{GenericImageView, ImageReader, Rgb};

fn main() -> anyhow::Result<()> {
    let x = std::env::args()
        .nth(1)
        .expect("Usage: ./k_means_test <file>");

    let image = ImageReader::open(x)
        .context("reading image")?
        .decode()
        .context("Decoding image")?;

    let k = 4;
    let pixels = image
        .pixels()
        .map(|(_, _, x)| Rgb::<u8>([x[0], x[1], x[2]]))
        .collect::<Vec<_>>();

    let clusters = gen_rp_rs::k_means::k_means(k, &pixels);

    let mut image = image.into_rgb8();

    let w = image.width();
    dbg!(&clusters);
    for (x, y, px) in image.enumerate_pixels_mut() {
        // *px = if x < w / 2 { clusters[0] } else { clusters[1] }
        let next = closest(*px, &clusters);
        *px = next;
    }

    image.save("out.png")?;

    Ok(())
}
