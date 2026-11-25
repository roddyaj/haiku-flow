import { openUrl } from "@tauri-apps/plugin-opener";
import type { ReactNode } from "react";

interface Props {
  href: string;
  title?: string;
  className?: string;
  children: ReactNode;
}

/** Link that opens in the system browser. */
export function Link({ href, title, className, children }: Props) {
  return (
    <a
      href={href}
      title={title}
      className={className}
      onClick={(e) => {
        e.preventDefault();
        openUrl(href).catch(console.error);
      }}
    >
      {children}
    </a>
  );
}
