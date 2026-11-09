//! The Mission framework's breakpoint interpolator
//! (`raprsrnunraeegybghbdzrvqypyotzm(short, float*, short)`): `table` holds
//! (x, y) pairs with ascending x; below the first x it returns the first y,
//! from the last x on the last y, in between it interpolates linearly — all
//! in single precision, like the original.

pub fn breakpoint(x: i16, table: &[f32]) -> f32 {
    let n = table.len();
    let f = x as i32 as f32;
    if table[0] < f {
        if f < table[n - 2] {
            let mut s = 0;
            while s < n && table[s] <= f {
                s += 2;
            }
            ((f - table[s - 2]) / (table[s] - table[s - 2])) * (table[s + 1] - table[s - 1]) + table[s - 1]
        } else {
            table[n - 1]
        }
    } else {
        table[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_and_clamps() {
        let t = [0.0, 100.0, 8.0, 100.0, 30.0, 55.0, 100.0, 10.0, 127.0, 0.0];
        assert_eq!(breakpoint(-3, &t), 100.0);
        assert_eq!(breakpoint(19, &t), 77.5);
        assert_eq!(breakpoint(127, &t), 0.0);
    }
}
