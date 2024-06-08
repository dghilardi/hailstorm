use std::f64::consts::PI;
use std::ops::{Mul, Sub};

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
