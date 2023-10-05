import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "AIlly",
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
        <link
          rel="stylesheet"
          type="text/css"
          href="https://unpkg.com/@davidsouther/jiffies-css"
        />
        <script
          async
          src="https://unpkg.com/@davidsouther/jiffies-css/accessibility.js"
        ></script>
      </head>
      <body style={{ padding: "0 var(--spacing-block-horizontal)" }}>
        {children}
      </body>
    </html>
  );
}
