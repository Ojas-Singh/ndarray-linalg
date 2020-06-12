//! # Least Squares
//!
//! Compute a least-squares solution to the equation Ax = b.
//! Compute a vector x such that the 2-norm ``|b - A x|`` is minimized.
//!
//! Finding the least squares solutions is implemented as traits, meaning
//! that to solve `A x = b` for a matrix `A` and a RHS `b`, we call
//! `let result = A.least_squares(&b);`. This returns a `result` of
//! type `LeastSquaresResult`, the solution for the least square problem
//! is in `result.solution`.
//!
//! There are three traits, `LeastSquaresSvd` with the method `least_squares`,
//! which operates on immutable references, `LeastSquaresInto` with the method
//! `least_squares_into`, which takes ownership over both the array `A` and the
//! RHS `b` and `LeastSquaresSvdInPlace` with the method `least_squares_in_place`,
//! which operates on mutable references for `A` and `b` and destroys these when
//! solving the least squares problem. `LeastSquaresSvdInto` and
//! `LeastSquaresSvdInPlace` avoid an extra allocation for `A` and `b` which
//! `LeastSquaresSvd` has do perform to preserve the values in `A` and `b`.
//!
//! All methods use the Lapacke family of methods `*gelsd` which solves the least
//! squares problem using the SVD with a divide-and-conquer strategy.
//!
//! The traits are implemented for value types `f32`, `f64`, `c32` and `c64`
//! and vector or matrix right-hand-sides (`ArrayBase<S, Ix1>` or `ArrayBase<S, Ix2>`).
//!
//! ## Example
//! ```rust
//! use approx::AbsDiffEq; // for abs_diff_eq
//! use ndarray::{Array1, Array2};
//! use ndarray_linalg::{LeastSquaresSvd, LeastSquaresSvdInto, LeastSquaresSvdInPlace};
//!
//! let a: Array2<f64> = array![
//!     [1., 1., 1.],
//!     [2., 3., 4.],
//!     [3., 5., 2.],
//!     [4., 2., 5.],
//!     [5., 4., 3.]
//! ];
//! // solving for a single right-hand side
//! let b: Array1<f64> = array![-10., 12., 14., 16., 18.];
//! let expected: Array1<f64> = array![2., 1., 1.];
//! let result = a.least_squares(&b).unwrap();
//! assert!(result.solution.abs_diff_eq(&expected, 1e-12));
//!
//! // solving for two right-hand sides at once
//! let b_2: Array2<f64> =
//!     array![[-10., -3.], [12., 14.], [14., 12.], [16., 16.], [18., 16.]];
//! let expected_2: Array2<f64> = array![[2., 1.], [1., 1.], [1., 2.]];
//! let result_2 = a.least_squares(&b_2).unwrap();
//! assert!(result_2.solution.abs_diff_eq(&expected_2, 1e-12));
//!
//! // using `least_squares_in_place` which overwrites its arguments
//! let mut a_3 = a.clone();
//! let mut b_3 = b.clone();
//! let result_3 = a_3.least_squares_in_place(&mut b_3).unwrap();
//!
//! // using `least_squares_into` which consumes its arguments
//! let result_4 = a.least_squares_into(b).unwrap();
//! // `a` and `b` have been moved, no longer valid
//! ```

use ndarray::{s, Array, Array1, Array2, ArrayBase, Axis, Data, DataMut, Ix1, Ix2};

use crate::error::*;
use crate::lapack::least_squares::*;
use crate::layout::*;
use crate::types::*;

pub trait Ix1OrIx2<E: Scalar> {
    type ScalarOrArray1;
}

impl<E: Scalar> Ix1OrIx2<E> for Ix1 {
    type ScalarOrArray1 = E;
}

impl<E: Scalar> Ix1OrIx2<E> for Ix2 {
    type ScalarOrArray1 = Array1<E>;
}

/// Result of a LeastSquares computation
///
/// Takes two type parameters, `E`, the element type of the matrix
/// (one of `f32`, `f64`, `c32` or `c64`) and `I`, the dimension of
/// b in the equation `Ax = b` (one of `Ix1` or `Ix2`). If `I` is `Ix1`,
/// the  right-hand-side (RHS) is a `n x 1` column vector and the solution
/// is a `m x 1` column vector. If `I` is `Ix2`, the RHS is a `n x k` matrix
/// (which can be seen as solving `Ax = b` k times for different b) and
/// the solution is a `m x k` matrix.
pub struct LeastSquaresResult<E: Scalar, I: Ix1OrIx2<E>> {
    /// The singular values of the matrix A in `Ax = b`
    pub singular_values: Array1<E::Real>,
    /// The solution vector or matrix `x` which is the best
    /// solution to `Ax = b`, i.e. minimizing the 2-norm `||b - Ax||`
    pub solution: Array<E, I>,
    /// The rank of the matrix A in `Ax = b`
    pub rank: i32,
    /// If n < m and rank(A) == n, the sum of squares
    /// If b is a (m x 1) vector, this is a single value
    /// If b is a m x k matrix, this is a k x 1 column vector
    pub residual_sum_of_squares: Option<I::ScalarOrArray1>,
}
/// Solve least squares for immutable references
pub trait LeastSquaresSvd<D, E, I>
where
    D: Data<Elem = E>,
    E: Scalar + Lapack,
    I: Ix1OrIx2<E>,
{
    /// Solve a least squares problem of the form `Ax = rhs`
    /// by calling `A.least_squares(&rhs)`. `A` and `rhs`
    /// are unchanged.
    ///
    /// `A` and `rhs` must have the same layout, i.e. they must
    /// be both either row- or column-major format, otherwise a
    /// `IncompatibleShape` error is raised.
    fn least_squares(&self, rhs: &ArrayBase<D, I>) -> Result<LeastSquaresResult<E, I>>;
}

/// Solve least squares for owned matrices
pub trait LeastSquaresSvdInto<D, E, I>
where
    D: Data<Elem = E>,
    E: Scalar + Lapack,
    I: Ix1OrIx2<E>,
{
    /// Solve a least squares problem of the form `Ax = rhs`
    /// by calling `A.least_squares(rhs)`, consuming both `A`
    /// and `rhs`. This uses the memory location of `A` and
    /// `rhs`, which avoids some extra memory allocations.
    ///
    /// `A` and `rhs` must have the same layout, i.e. they must
    /// be both either row- or column-major format, otherwise a
    /// `IncompatibleShape` error is raised.
    fn least_squares_into(self, rhs: ArrayBase<D, I>) -> Result<LeastSquaresResult<E, I>>;
}

/// Solve least squares for mutable references, overwriting
/// the input fields in the process
pub trait LeastSquaresSvdInPlace<D, E, I>
where
    D: Data<Elem = E>,
    E: Scalar + Lapack,
    I: Ix1OrIx2<E>,
{
    /// Solve a least squares problem of the form `Ax = rhs`
    /// by calling `A.least_squares(&mut rhs)`, overwriting both `A`
    /// and `rhs`. This uses the memory location of `A` and
    /// `rhs`, which avoids some extra memory allocations.
    ///
    /// `A` and `rhs` must have the same layout, i.e. they must
    /// be both either row- or column-major format, otherwise a
    /// `IncompatibleShape` error is raised.
    fn least_squares_in_place(
        &mut self,
        rhs: &mut ArrayBase<D, I>,
    ) -> Result<LeastSquaresResult<E, I>>;
}

/// Solve least squares for immutable references and a single
/// column vector as a right-hand side.
/// `E` is one of `f32`, `f64`, `c32`, `c64`. `D` can be any
/// valid representation for `ArrayBase`.
impl<E, D> LeastSquaresSvd<D, E, Ix1> for ArrayBase<D, Ix2>
where
    E: Scalar + Lapack + LeastSquaresSvdDivideConquer_,
    D: Data<Elem = E>,
{
    /// Solve a least squares problem of the form `Ax = rhs`
    /// by calling `A.least_squares(&rhs)`, where `rhs` is a
    /// single column vector. `A` and `rhs` are unchanged.
    ///
    /// `A` and `rhs` must have the same layout, i.e. they must
    /// be both either row- or column-major format, otherwise a
    /// `IncompatibleShape` error is raised.
    fn least_squares(&self, rhs: &ArrayBase<D, Ix1>) -> Result<LeastSquaresResult<E, Ix1>> {
        let a = self.to_owned();
        let b = rhs.to_owned();
        a.least_squares_into(b)
    }
}

/// Solve least squares for immutable references and matrix
/// (=mulitipe vectors) as a right-hand side.
/// `E` is one of `f32`, `f64`, `c32`, `c64`. `D` can be any
/// valid representation for `ArrayBase`.
impl<E, D> LeastSquaresSvd<D, E, Ix2> for ArrayBase<D, Ix2>
where
    E: Scalar + Lapack + LeastSquaresSvdDivideConquer_,
    D: Data<Elem = E>,
{
    /// Solve a least squares problem of the form `Ax = rhs`
    /// by calling `A.least_squares(&rhs)`, where `rhs` is
    /// matrix. `A` and `rhs` are unchanged.
    ///
    /// `A` and `rhs` must have the same layout, i.e. they must
    /// be both either row- or column-major format, otherwise a
    /// `IncompatibleShape` error is raised.
    fn least_squares(&self, rhs: &ArrayBase<D, Ix2>) -> Result<LeastSquaresResult<E, Ix2>> {
        let a = self.to_owned();
        let b = rhs.to_owned();
        a.least_squares_into(b)
    }
}

/// Solve least squares for owned values and a single
/// column vector as a right-hand side. The matrix and the RHS
/// vector are consumed.
///
/// `E` is one of `f32`, `f64`, `c32`, `c64`. `D` can be any
/// valid representation for `ArrayBase`.
impl<E, D> LeastSquaresSvdInto<D, E, Ix1> for ArrayBase<D, Ix2>
where
    E: Scalar + Lapack + LeastSquaresSvdDivideConquer_,
    D: DataMut<Elem = E>,
{
    /// Solve a least squares problem of the form `Ax = rhs`
    /// by calling `A.least_squares(rhs)`, where `rhs` is a
    /// single column vector. `A` and `rhs` are consumed.
    ///
    /// `A` and `rhs` must have the same layout, i.e. they must
    /// be both either row- or column-major format, otherwise a
    /// `IncompatibleShape` error is raised.
    fn least_squares_into(
        mut self,
        mut rhs: ArrayBase<D, Ix1>,
    ) -> Result<LeastSquaresResult<E, Ix1>> {
        self.least_squares_in_place(&mut rhs)
    }
}

/// Solve least squares for owned values and a matrix
/// as a right-hand side. The matrix and the RHS matrix
/// are consumed.
///
/// `E` is one of `f32`, `f64`, `c32`, `c64`. `D` can be any
/// valid representation for `ArrayBase`.
impl<E, D> LeastSquaresSvdInto<D, E, Ix2> for ArrayBase<D, Ix2>
where
    E: Scalar + Lapack + LeastSquaresSvdDivideConquer_,
    D: DataMut<Elem = E>,
{
    /// Solve a least squares problem of the form `Ax = rhs`
    /// by calling `A.least_squares(rhs)`, where `rhs` is a
    /// matrix. `A` and `rhs` are consumed.
    ///
    /// `A` and `rhs` must have the same layout, i.e. they must
    /// be both either row- or column-major format, otherwise a
    /// `IncompatibleShape` error is raised.
    fn least_squares_into(
        mut self,
        mut rhs: ArrayBase<D, Ix2>,
    ) -> Result<LeastSquaresResult<E, Ix2>> {
        self.least_squares_in_place(&mut rhs)
    }
}

/// Solve least squares for mutable references and a vector
/// as a right-hand side. Both values are overwritten in the
/// call.
///
/// `E` is one of `f32`, `f64`, `c32`, `c64`. `D` can be any
/// valid representation for `ArrayBase`.
impl<E, D> LeastSquaresSvdInPlace<D, E, Ix1> for ArrayBase<D, Ix2>
where
    E: Scalar + Lapack + LeastSquaresSvdDivideConquer_,
    D: DataMut<Elem = E>,
{
    /// Solve a least squares problem of the form `Ax = rhs`
    /// by calling `A.least_squares(rhs)`, where `rhs` is a
    /// vector. `A` and `rhs` are overwritten in the call.
    ///
    /// `A` and `rhs` must have the same layout, i.e. they must
    /// be both either row- or column-major format, otherwise a
    /// `IncompatibleShape` error is raised.
    fn least_squares_in_place(
        &mut self,
        rhs: &mut ArrayBase<D, Ix1>,
    ) -> Result<LeastSquaresResult<E, Ix1>> {
        let a_layout = self.layout()?;
        let LeastSquaresOutput::<E> {
            singular_values,
            rank,
        } = unsafe {
            <E as LeastSquaresSvdDivideConquer_>::least_squares(
                a_layout,
                self.as_allocated_mut()?,
                rhs.as_slice_memory_order_mut()
                    .ok_or_else(|| LinalgError::MemoryNotCont)?,
            )?
        };

        let (m, n) = (self.shape()[0], self.shape()[1]);
        let solution = rhs.slice(s![0..n]).to_owned();
        let residual_sum_of_squares = compute_residual_scalar(m, n, rank, &rhs);
        Ok(LeastSquaresResult {
            solution,
            singular_values: Array::from_shape_vec((singular_values.len(),), singular_values)?,
            rank,
            residual_sum_of_squares,
        })
    }
}

fn compute_residual_scalar<E: Scalar, D: Data<Elem = E>>(
    m: usize,
    n: usize,
    rank: i32,
    b: &ArrayBase<D, Ix1>,
) -> Option<E> {
    if m < n || n != rank as usize {
        return None;
    }
    Some(b.slice(s![n..]).mapv(|x| x.powi(2)).sum())
}

/// Solve least squares for mutable references and a matrix
/// as a right-hand side. Both values are overwritten in the
/// call.
///
/// `E` is one of `f32`, `f64`, `c32`, `c64`. `D` can be any
/// valid representation for `ArrayBase`.
impl<E, D> LeastSquaresSvdInPlace<D, E, Ix2> for ArrayBase<D, Ix2>
where
    E: Scalar + Lapack + LeastSquaresSvdDivideConquer_,
    D: DataMut<Elem = E>,
{
    /// Solve a least squares problem of the form `Ax = rhs`
    /// by calling `A.least_squares(rhs)`, where `rhs` is a
    /// matrix. `A` and `rhs` are overwritten in the call.
    ///
    /// `A` and `rhs` must have the same layout, i.e. they must
    /// be both either row- or column-major format, otherwise a
    /// `IncompatibleShape` error is raised.
    fn least_squares_in_place(
        &mut self,
        rhs: &mut ArrayBase<D, Ix2>,
    ) -> Result<LeastSquaresResult<E, Ix2>> {
        let a_layout = self.layout()?;
        let rhs_layout = rhs.layout()?;
        let LeastSquaresOutput::<E> {
            singular_values,
            rank,
        } = unsafe {
            <E as LeastSquaresSvdDivideConquer_>::least_squares_nrhs(
                a_layout,
                self.as_allocated_mut()?,
                rhs_layout,
                rhs.as_allocated_mut()?,
            )?
        };

        let solution: Array2<E> = rhs.slice(s![..self.shape()[1], ..]).to_owned();
        let singular_values = Array::from_shape_vec((singular_values.len(),), singular_values)?;
        let (m, n) = (self.shape()[0], self.shape()[1]);
        let residual_sum_of_squares = compute_residual_array1(m, n, rank, &rhs);
        Ok(LeastSquaresResult {
            solution,
            singular_values,
            rank,
            residual_sum_of_squares,
        })
    }
}

fn compute_residual_array1<E: Scalar, D: Data<Elem = E>>(
    m: usize,
    n: usize,
    rank: i32,
    b: &ArrayBase<D, Ix2>,
) -> Option<Array1<E>> {
    if m < n || n != rank as usize {
        return None;
    }
    Some(b.slice(s![n.., ..]).mapv(|x| x.powi(2)).sum_axis(Axis(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::AbsDiffEq;
    use ndarray::{Array1, Array2};

    /// This test case is taken from the netlib documentation at
    /// https://www.netlib.org/lapack/lapacke.html#_calling_code_dgels_code
    /// It tests the example with the first vector on the right hand side
    #[test]
    fn netlib_lapack_example_for_dgels_1() {
        let a: Array2<f64> = array![
            [1., 1., 1.],
            [2., 3., 4.],
            [3., 5., 2.],
            [4., 2., 5.],
            [5., 4., 3.]
        ];
        let b: Array1<f64> = array![-10., 12., 14., 16., 18.];
        let expected: Array1<f64> = array![2., 1., 1.];
        let result = a.least_squares(&b).unwrap();
        assert!(result.solution.abs_diff_eq(&expected, 1e-12));

        let residual = b - a.dot(&result.solution);
        let resid_ssq = result.residual_sum_of_squares.unwrap();
        assert!((resid_ssq - residual.dot(&residual)).abs() < 1e-12);
    }

    /// This test case is taken from the netlib documentation at
    /// https://www.netlib.org/lapack/lapacke.html#_calling_code_dgels_code
    /// It tests the example with the second vector on the right hand side
    #[test]
    fn netlib_lapack_example_for_dgels_2() {
        let a: Array2<f64> = array![
            [1., 1., 1.],
            [2., 3., 4.],
            [3., 5., 2.],
            [4., 2., 5.],
            [5., 4., 3.]
        ];
        let b: Array1<f64> = array![-3., 14., 12., 16., 16.];
        let expected: Array1<f64> = array![1., 1., 2.];
        let result = a.least_squares(&b).unwrap();
        assert!(result.solution.abs_diff_eq(&expected, 1e-12));

        let residual = b - a.dot(&result.solution);
        let resid_ssq = result.residual_sum_of_squares.unwrap();
        assert!((resid_ssq - residual.dot(&residual)).abs() < 1e-12);
    }

    /// This test case is taken from the netlib documentation at
    /// https://www.netlib.org/lapack/lapacke.html#_calling_code_dgels_code
    /// It tests that the least squares solution works as expected for
    /// multiple right hand sides
    #[test]
    fn netlib_lapack_example_for_dgels_nrhs() {
        let a: Array2<f64> = array![
            [1., 1., 1.],
            [2., 3., 4.],
            [3., 5., 2.],
            [4., 2., 5.],
            [5., 4., 3.]
        ];
        let b: Array2<f64> =
            array![[-10., -3.], [12., 14.], [14., 12.], [16., 16.], [18., 16.]];
        let expected: Array2<f64> = array![[2., 1.], [1., 1.], [1., 2.]];
        let result = a.least_squares(&b).unwrap();
        assert!(result.solution.abs_diff_eq(&expected, 1e-12));

        let residual = &b - &a.dot(&result.solution);
        let residual_ssq = residual.mapv(|x| x.powi(2)).sum_axis(Axis(0));
        assert!(result.residual_sum_of_squares.unwrap().abs_diff_eq(&residual_ssq, 1e-12));
    }
}
