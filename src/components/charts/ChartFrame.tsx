import { useEffect, useRef, useState, type ReactNode } from "react";
import { ResponsiveContainer } from "recharts";

interface Props {
  children: ReactNode;
}

/** Measured host for responsive charts. Recharts logs a width(0)/height(0)
 * warning whenever ResponsiveContainer measures a zero-sized box, which
 * happens for charts mounted under hidden tab panels or kept-alive pages.
 * This frame measures the box itself and only mounts the chart — at a fixed
 * numeric size, bypassing recharts' detector — while the box is visible. */
export function ChartFrame({ children }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const measure = () => {
      const rect = host.getBoundingClientRect();
      const next =
        rect.width > 0 && rect.height > 0
          ? { width: Math.round(rect.width), height: Math.round(rect.height) }
          : null;
      setSize((prev) =>
        prev?.width === next?.width && prev?.height === next?.height ? prev : next,
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  return (
    <div ref={hostRef} className="h-full w-full">
      {size && (
        <ResponsiveContainer width={size.width} height={size.height}>
          {children}
        </ResponsiveContainer>
      )}
    </div>
  );
}
