import { cn } from "../utils";
import type { PriceReading } from "./sessionSpend";

/**
 * A price beside the tokens that bought it.
 *
 * The `≈` is load-bearing rather than decorative: it is what separates this
 * lens's own arithmetic from a figure a harness charged, and it sits in the
 * amount itself so a reader who never reaches the basis note is still not shown
 * an estimate as a receipt. The accessible name says the same thing in words,
 * since a screen reader may voice the sign as "almost equal to" or drop it.
 */
export function PriceAmount({ reading, className }: { reading: PriceReading; className?: string }) {
  return (
    <span
      aria-label={`${reading.approximate ? "Estimated price" : "Price"} ${reading.label.replace(
        "≈",
        "",
      )}`}
      className={cn("shrink-0 text-foreground-muted", className)}
    >
      {reading.label}
    </span>
  );
}
