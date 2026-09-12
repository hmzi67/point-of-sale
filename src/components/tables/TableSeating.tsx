/** A top-down table with a chair for each seat; large banquet capacities
 * use a count for the extra chairs to keep the floor view readable. */
export function TableSeating({ seats }: { seats: number }) {
  const visible = Math.min(24, Math.max(1, Math.floor(seats)));
  const top = Math.ceil(visible / 2);
  const bottom = Math.floor(visible / 2);
  const width = Math.max(100, top * 30);
  const chairRow = (count: number, y: number) => Array.from({ length: count }, (_, i) => (
    <rect key={`${y}-${i}`} x={30 + ((i + 0.5) * width) / count - 10} y={y}
      width={20} height={16} rx={5} fill="currentColor" fillOpacity={0.2}
      stroke="currentColor" strokeWidth={1.5} />
  ));
  return (
    <svg viewBox={`0 0 ${width + 60} 156`} className="h-36 w-full" aria-hidden="true">
      {chairRow(top, 10)}
      <rect x={30} y={34} width={width} height={88} rx={visible <= 4 ? 24 : 14}
        fill="currentColor" fillOpacity={0.08} stroke="currentColor" strokeWidth={2} />
      <rect x={36} y={40} width={width - 12} height={76} rx={visible <= 4 ? 19 : 9}
        fill="none" stroke="currentColor" strokeOpacity={0.15} />
      <text x={(width + 60) / 2} y={76} textAnchor="middle" dominantBaseline="middle"
        fill="currentColor" fontSize={24} fontWeight={600}>{seats}</text>
      <text x={(width + 60) / 2} y={98} textAnchor="middle" fill="currentColor" fontSize={11}>
        {seats > visible ? `seats · +${seats - visible} chairs` : seats === 1 ? "seat" : "seats"}
      </text>
      {chairRow(bottom, 130)}
    </svg>
  );
}
