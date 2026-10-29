import { useState } from 'react';

type Props = { label: string };

export function Badge({ label }: Props) {
  const [count] = useState(0);
  return (
    <span className="badge" title={label}>
      {label} {count}
    </span>
  );
}
