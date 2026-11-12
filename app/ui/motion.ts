// Motion tokens (DESIGN.md): durations in ms, and the class each control
// family uses for its state changes. Reduced motion turns scales off.

export const duration = { instant: 100, fast: 150, base: 200, slow: 300 } as const;

/** A pressable control: colors wash in, the control sinks to 0.97. */
export const press =
  "transition duration-150 ease-out active:scale-[0.97] motion-reduce:transition-none motion-reduce:active:scale-100";

/** A row: colors only, rows deepen instead of scaling. */
export const wash = "transition-colors duration-100 ease-out motion-reduce:transition-none";
