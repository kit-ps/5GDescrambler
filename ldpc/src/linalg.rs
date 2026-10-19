use ndarray::{s, Array2, LinalgScalar};

pub fn row_echelon_form<A: LinalgScalar + PartialEq>(array: &mut Array2<A>) {
    let (n, m) = array.dim();

    let mut j = 0;
    let mut k = 0;
    while j < m && k < n {
        // Find non-zero element in current column, at or below row k
        let Some(s) = array
            .slice(s![k.., j])
            .iter()
            .enumerate()
            .find_map(|(t, x)| if x.is_zero() { None } else { Some(k + t) })
        else {
            // All the elements at or below row k are zero. Done with this
            // column.
            j += 1;
            continue;
        };

        if s != k {
            // Swap rows s and k
            for t in j..m {
                array.swap([s, t], [k, t]);
            }
        }

        let x = array[[k, j]];

        // Subtract to rows below to make zeros below row k
        for t in (k + 1)..n {
            let y = array[[t, j]];
            if !y.is_zero() {
                // avoid calculations if we're subtracting zero
                for u in j..m {
                    array[[t, u]] = array[[t, u]] - y * array[[k, u]] / x;
                }
            }
        }

        j += 1;
        k += 1;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::gf2::GF2;
    use ndarray::arr2;
    use num_traits::{One, Zero};

    #[test]
    fn row_echelon() {
        let i = GF2::one();
        let o = GF2::zero();
        let mut a = arr2(&[
            [i, i, o, o, i, o, i, o, i],
            [i, o, o, i, i, i, o, i, o],
            [i, i, o, o, o, i, i, o, i],
        ]);
        row_echelon_form(&mut a);
        let expected = arr2(&[
            [i, i, o, o, i, o, i, o, i],
            [o, i, o, i, o, i, i, i, i],
            [o, o, o, o, i, i, o, o, o],
        ]);
        assert_eq!(&a, &expected);
    }
}
