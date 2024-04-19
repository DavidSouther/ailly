import NavHeader from "@/components/NavHeader";
import type { Metadata } from "next";
import "./globals.css";
import styles from "./layout.module.css";

export const metadata: Metadata = {
  title: "Ailly",
  description: "AI Writing Ally - by David Souther",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <head>
        {/* <link
          rel="stylesheet"
          type="text/css"
          href="https://unpkg.com/@davidsouther/jiffies-css"
        /> */}
        {/* <script
          async
          src="https://unpkg.com/@davidsouther/jiffies-css/accessibility.js"
        ></script> */}
      </head>
      <body className={`container ${styles.body}`}>
        <NavHeader title="Ailly" />
        <main>{children}</main>
      </body>
    </html>
  );
}
