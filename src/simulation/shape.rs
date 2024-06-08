use std::f64::consts::PI;
use std::ops::{Mul, Sub};

/// Parses a given string representation of a mathematical function into a callable function.
///
/// This function leverages the `meval` crate to parse and compile a string into a function
/// that takes a `f64` as input and returns a `f64`. It extends the parsing context with custom
/// shape functions (`rect`, `tri`, `step`, `trapz`, `costrapz`) before parsing, allowing these
/// to be used within the input string.
///
/// # Parameters
///
/// * `fun`: A `String` representing the mathematical function to be parsed. This string can
/// include standard mathematical operations, numbers, and the custom shape functions defined
/// within this function.
///
/// # Returns
///
/// A `Result` which is:
/// - `Ok`: Contains the parsed function as `impl Fn(f64) -> f64`. This function can be called
/// with a `f64` argument to evaluate the expression at that point.
/// - `Err`: An error of type `meval::Error` if the string cannot be parsed into a valid
/// mathematical expression or function.
///
/// # Custom Shape Functions
///
/// - `rect(x)`: Returns 1.0 for |x| < 0.5, 0.5 for |x| = 0.5, and 0.0 otherwise.
/// - `tri(x)`: Returns 1.0 - |x| for |x| < 1.0, and 0.0 otherwise.
/// - `step(x)`: Returns 1.0 for x > 0, 0.5 for x = 0, and 0.0 otherwise.
/// - `trapz(x, b_low, b_sup)`: Trapezoidal function that depends on x and bounds b_low and b_sup.
/// - `costrapz(x, b_low, b_sup)`: Cosine-tapered trapezoidal function, also depending on x and bounds.
///
/// # Examples
///
/// ```
/// use hailstorm::simulation::shape::parse_shape_fun;
///
/// let fun_str = "rect(t) + tri(t - 1)".to_string();
/// let parsed_fun = parse_shape_fun(fun_str).unwrap();
/// println!("{}", parsed_fun(0.5)); // Evaluates the function at t = 0.5
/// ```
///
/// # Errors
///
/// This function will return an error if the string cannot be parsed into a valid mathematical
/// expression, including syntax errors or unrecognized function names (outside of the custom
/// shape functions provided).
///
/// # Panics
///
/// This function does not panic under normal circumstances. However, misuse of the `meval` crate
/// or invalid manipulation of the context might lead to unexpected behavior.
///
/// # Safety
///
/// This function is safe to use as it does not involve unsafe code blocks. The safety and correctness
/// of the returned function depend on the `meval` crate's ability to parse and evaluate mathematical
/// expressions safely.
pub fn parse_shape_fun(fun: String) -> Result<impl Fn(f64) -> f64, meval::Error> {
    let mut ctx = meval::Context::new(); // built-ins
    ctx.func("rect", |x| {
        if x.abs() > 0.5 {
            0.0
        } else if x.abs() == 0.5 {
            0.5
        } else {
            1.0
        }
    })
    .func("tri", |x| if x.abs() < 1.0 { 1.0 - x.abs() } else { 0.0 })
    .func("step", |x| {
        if x < 0.0 {
            0.0
        } else if x == 0.0 {
            0.5
        } else {
            1.0
        }
    })
    .func3("trapz", |x, b_low, b_sup| {
        if x.abs() > b_low / 2.0 {
            0.0
        } else if x.abs() < b_sup / 2.0 {
            1.0
        } else {
            (x.abs() * 2.0 - b_low) / (b_sup - b_low)
        }
    })
    .func3("costrapz", |x, b_low, b_sup| {
        if x.abs() > b_low / 2.0 {
            0.0
        } else if x.abs() < b_sup / 2.0 {
            1.0
        } else {
            x.abs()
                .sub(b_sup / 2.0)
                .mul(PI / (b_low - b_sup))
                .cos()
                .powi(2)
        }
    });

    let expr: meval::Expr = fun.parse()?;
    expr.bind_with_context(ctx, "t")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_trapz_parse() {
        for f_name in [
            "trapz(t,2,1)",
            "costrapz(t,2,1)",
            "tri(t)",
            "rect(t)",
            "step(t)",
        ] {
            let fun = parse_shape_fun(String::from(f_name)).expect("Error parsing fun");

            let coord = (0..=512)
                .into_iter()
                .map(|x| {
                    let y = 224.0 - fun(x as f64 / 256.0 - 1.0) * 192.0;
                    format!("{},{:.2}", x, y)
                })
                .collect::<Vec<_>>()
                .join(" ");
            println!("{f_name}: {coord}");
        }
    }
}
