import { MatrixStarlightCanvas, type MatrixStarlightVariant } from "./MatrixStarlightCanvas";

/** Decorative only; mounting is owned by the visible, readable connection card. */
export function StarlightLayer({ variant }: { variant: MatrixStarlightVariant }) {
  return (
    <span className="asb-starlight" data-variant={variant} aria-hidden="true">
      <MatrixStarlightCanvas variant={variant} />
      <span className="asb-starlight-glow" />
    </span>
  );
}
