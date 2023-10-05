export default function NavHeader({ title }: { title: string }) {
  return (
    <header>
      <nav>
        <ul>
          <li>
            <h1>{title}</h1>
          </li>
        </ul>
      </nav>
    </header>
  );
}
