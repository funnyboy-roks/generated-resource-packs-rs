use core::f64;

use image::Rgb;

type Point = Rgb<u8>;

fn rand_point() -> Point {
    Rgb::<u8>([rand::random(), rand::random(), rand::random()])
}

pub fn dist_sq(p1: Point, p2: Point) -> f64 {
    (p1[0] as f64 - p2[0] as f64).powi(2)
        + (p1[1] as f64 - p2[1] as f64).powi(2)
        + (p1[2] as f64 - p2[2] as f64).powi(2)
}

fn dist(p1: Point, p2: Point) -> f64 {
    dist_sq(p1, p2).sqrt()
}

fn calculate_centroid(points: Vec<Point>) -> Option<Point> {
    if points.is_empty() {
        eprintln!("no points");
        if rand::random_bool(0.25) {
            return Some(rand_point());
        }
        return None;
    }

    let mut r = 0u32;
    let mut g = 0u32;
    let mut b = 0u32;

    for p in &points {
        r += p[0] as u32;
        g += p[1] as u32;
        b += p[2] as u32;
    }

    let r = (r / (points.len() as u32)) as u8;
    let g = (g / (points.len() as u32)) as u8;
    let b = (b / (points.len() as u32)) as u8;

    Some(Rgb::<u8>([r, g, b]))
}

pub fn closest(p1: Point, points: &[Point]) -> Point {
    let mut min_dist = 100000.;
    let mut min_i = 0;

    for (i, p2) in points.iter().enumerate() {
        let d = dist_sq(p1, *p2);
        if d < min_dist {
            min_dist = d;
            min_i = i;
        }
    }

    points[min_i]
}

pub fn k_means(k: usize, points: &[Point]) -> Vec<Point> {
    let mut centroids = (0..k).map(|_| rand_point()).collect::<Vec<_>>();
    let mut converged = false;

    while !converged {
        let mut clusters = (0..k)
            .map(|_| Vec::<Point>::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();

        for point in points {
            let mut closest_index = 0;
            let mut min_dist = dist_sq(*point, centroids[0]);
            for (j, centroid) in centroids.iter().enumerate().skip(1) {
                let d = dist_sq(*point, *centroid);
                if d < min_dist {
                    min_dist = d;
                    closest_index = j;
                }
            }
            clusters[closest_index].push(*point);
        }

        let mut new_centroids = Vec::new();

        for cluster in clusters {
            if let Some(new_centroid) = calculate_centroid(cluster) {
                new_centroids.push(new_centroid);
            }
        }

        if new_centroids == centroids {
            converged = true;
        } else {
            centroids = new_centroids;
        }
    }

    centroids
}

// function kmeans(k, points) is
//     // Initialize centroids
//     centroids ← list of k starting centroids
//     converged ← false
//
//     while converged == false do
//         // Create empty clusters
//         clusters ← list of k empty lists
//
//         // Assign each point to the nearest centroid
//         for i ← 0 to length(points) - 1 do
//             point ← points[i]
//             closestIndex ← 0
//             minDistance ← distance(point, centroids[0])
//             for j ← 1 to k - 1 do
//                 d ← distance(point, centroids[j])
//                 if d < minDistance THEN
//                     minDistance ← d
//                     closestIndex ← j
//             clusters[closestIndex].append(point)
//
//         // Recalculate centroids as the mean of each cluster
//         newCentroids ← empty list
//         for i ← 0 to k - 1 do
//             newCentroid ← calculateCentroid(clusters[i])
//             newCentroids.append(newCentroid)
//
//         // Check for convergence
//         if newCentroids == centroids THEN
//             converged ← true
//         else
//             centroids ← newCentroids
//
//     return clusters
