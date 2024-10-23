

use std::marker::PhantomData;

/// chap2: chip
/// Prove knowing knowledge of three private inputs a, b, c
/// s.t:
///     d = a^2 * b^2 * c
///     e = c + d
///     out = e^3
use halo2_proofs::{
    arithmetic::Field,
    circuit::{AssignedCell, Layouter, SimpleFloorPlanner, Value},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Constraints, Error, Instance, Selector},
    poly::Rotation,
};

/// Circuit design:
// / | ins   |  a0   |  a1  |  a2  | s_cpx |
// / |-------|-------|------|------|-------|
// / |  out  |   a   |   b  |   c  |   1   |
// / |       |  out  |      |      |       |

#[derive(Debug, Clone)]
struct SimpleConfig {
    advice: [Column<Advice>; 3],
    instance: Column<Instance>,
    s_cpx: Selector,
}

#[derive(Clone)]
struct Number<F: Field>(AssignedCell<F, F>);

#[derive(Debug, Clone)]
struct SimpleChip<F: Field> {
    config: SimpleConfig,
    _marker: PhantomData<F>,
}

impl<F: Field> SimpleChip<F> {
    pub fn construct(config: SimpleConfig) -> Self {
        SimpleChip {
            config,
            _marker: PhantomData,
        }
    }
    pub fn configure(meta: &mut ConstraintSystem<F>) -> SimpleConfig {
        let advice = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];
        let instance = meta.instance_column();
        let constant = meta.fixed_column();

        meta.enable_equality(instance);
        meta.enable_constant(constant);
        for c in &advice {
            meta.enable_equality(*c);
        }
        let s_cpx = meta.selector();

        meta.create_gate("complex_gate", |meta| {
            let l = meta.query_advice(advice[0], Rotation::cur());
            let r = meta.query_advice(advice[1], Rotation::cur());
            let c = meta.query_advice(advice[2], Rotation::cur());
            let out = meta.query_advice(advice[0], Rotation::next());

            let s_cpx = meta.query_selector(s_cpx);

            let e = (l.clone() * r.clone()) * (l * r) * c.clone() + c;
            let e_cub = e.clone() * e.clone() * e.clone();
            Constraints::with_selector(s_cpx, vec![e_cub - out])
        });

        SimpleConfig {
            advice,
            instance,
            s_cpx,
        }
    }

    pub fn assign(
        &self,
        mut layouter: impl Layouter<F>,
        a: Value<F>,
        b: Value<F>,
        c: F,
    ) -> Result<Number<F>, Error> {
        layouter.assign_region(
            || "load private & witness",
            |mut region| {
                let mut offset = 0;
                let config = &self.config;
                config.s_cpx.enable(&mut region, offset)?; // Attention the positon of s_cpx to offset.

                let a_cell = region
                    .assign_advice(|| "private input a", self.config.advice[0], offset, || a)
                    .map(Number)?;
                let b_cell = region
                    .assign_advice(|| "private input b", self.config.advice[1], offset, || b)
                    .map(Number)?;
                let c_cell = region
                    .assign_advice_from_constant(
                        || "private input c",
                        self.config.advice[2],
                        offset,
                        c,
                    )
                    .map(Number)?;
                offset += 1;
                let e: Value<F> = (a_cell.0.value().copied() * b_cell.0.value().copied())   // a * b    = ab
                    * (a_cell.0.value().copied() * b_cell.0.value().copied()) // ab * ab  = absq
                    * c_cell.0.value().copied()                               // absq * c = d
                    + c_cell.0.value().copied(); // d + c    = e
                let e_cub = e * e * e; // e_cub    = e^3
                region
                    .assign_advice(|| "out", config.advice[0], offset, || e_cub)
                    .map(Number)
            },
        )
    }

    fn expose_public(
        &self,
        mut layouter: impl Layouter<F>,
        out: Number<F>,
        row: usize,
    ) -> Result<(), Error> {
        layouter.constrain_instance(out.0.cell(), self.config.instance, row)
    }
}

#[derive(Default)]
struct MyCircuit<F: Field> {
    c: F,
    a: Value<F>,
    b: Value<F>,
}

impl<F: Field> Circuit<F> for MyCircuit<F> {
    type Config = SimpleConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<F>) -> Self::Config {
        SimpleChip::configure(meta)
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<F>,
    ) -> Result<(), Error> {
        //assign witness
        let chip = SimpleChip::construct(config);
        let out = chip.assign(
            layouter.namespace(|| "complex ship"),
            self.a,
            self.b,
            self.c,
        )?;
        //expose public
        chip.expose_public(layouter, out, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo2_proofs::{dev::MockProver, pasta::Fp};

    fn circuit() -> (MyCircuit<Fp>, Fp) {
        // Prepare the private and public inputs to the circuit!
        let c = Fp::from(2);
        let a = Fp::from(2);
        let b = Fp::from(3);
        let e = c * a.square() * b.square() + c;
        let out = e.cube();
        println!("out=:{:?}", out);

        // Instantiate the circuit with the private inputs.
        (
            MyCircuit {
                c,
                a: Value::known(a),
                b: Value::known(b),
            },
            out,
        )
    }
    #[test]
    fn test_chap_2_exercise_5() {
        // ANCHOR: test-circuit
        // The number of rows in our circuit cannot exceed 2^k. Since our example
        // circuit is very small, we can pick a very small value here.
        let k = 5;
        let (circuit, out) = circuit();

        // Arrange the public input. We expose the multiplication result in row 0
        // of the instance column, so we position it there in our public inputs.
        let mut public_inputs = vec![out];

        // Given the correct public input, our circuit will verify.
        let prover = MockProver::run(k, &circuit, vec![public_inputs.clone()]).unwrap();
        assert_eq!(prover.verify(), Ok(()));

        // If we try some other public input, the proof will fail!
        public_inputs[0] += Fp::one();
        let prover = MockProver::run(k, &circuit, vec![public_inputs]).unwrap();
        assert!(prover.verify().is_err());
        println!("simple_ship success!")
        // ANCHOR_END: test-circuit
    }

    #[cfg(feature = "dev-graph")]
    #[test]
    fn plot_chap_2_exercise_5() {
        // Instantiate the circuit with the private inputs.
        let (circuit, c) = circuit();
        // Create the area you want to draw on.
        // Use SVGBackend if you want to render to .svg instead.
        use plotters::prelude::*;
        let root = BitMapBackend::new(
            "./circuit_layouter_plots/chap_2_exercise_5.png",
            (1024, 768),
        )
        .into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root
            .titled("chip-complex-gate", ("sans-serif", 60))
            .unwrap();

        halo2_proofs::dev::CircuitLayout::default()
            // You can optionally render only a section of the circuit.
            // .view_width(0..2)
            // .view_height(0..16)
            // You can hide labels, which can be useful with smaller areas.
            .show_labels(true)
            // Render the circuit onto your area!
            // The first argument is the size parameter for the circuit.
            .render(4, &circuit, &root)
            .unwrap();
    }
}
