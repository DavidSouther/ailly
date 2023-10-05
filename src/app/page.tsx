import M from "@/components/M";
import Link from "next/link";

export default function Home() {
  return (
    <>
      <M>Get started by editing files in the `/content` directory.</M>
      <p>
        Then, see your content and trigger generating updates by navigating to
        the <Link href="/content">/content</Link> route.
      </p>
    </>
  );
}
