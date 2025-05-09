"use client";
import type { ReactNode } from "react";

export const Card = ({
  className = "",
  header,
  children,
  footer,
}: {
  className?: string;
  header?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
}) => (
  <article className={className}>
    <>
      {header && (
        <header>
          <h3>{header}</h3>
        </header>
      )}
      <main>{children}</main>
      {footer && <footer>{footer}</footer>}
    </>
  </article>
);
