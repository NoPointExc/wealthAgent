const ALL_COLORS = [
  '#ef4444', '#991b1b', '#fca5a5', '#f43f5e', '#9f1239', 
  '#ec4899', '#9d174d', '#f97316', '#9a3412', '#f59e0b', 
  '#b45309', '#fcd34d', '#eab308', '#854d0e', '#84cc16', 
  '#3f6212', '#22c55e', '#166534', '#86efac', '#10b981', 
  '#065f46', '#14b8a6', '#115e59', '#06b6d4', '#155e75', 
  '#0ea5e9', '#075985', '#3b82f6', '#1e40af', '#93c5fd', 
  '#6366f1', '#3730a3', '#8b5cf6', '#5b21b6', '#c4b5fd', 
  '#a855f7', '#6b21a8', '#d946ef', '#86198f', '#64748b'
];

export const getAccountColor = (index: number, isLiability: boolean): string => {
  // To maximize the visual distance between sequential accounts, we step through the array 
  // by a prime number (17) that is coprime with the length (40). 
  // This effectively scrambles the adjacent colors while guaranteeing we use every color 
  // exactly once before repeating.
  const step = 17;
  
  // We offset liabilities by a fixed amount so they don't start at the same exact color 
  // as the first asset, providing cross-section uniqueness early on.
  const offset = isLiability ? 7 : 0; 
  
  const colorIndex = ((index * step) + offset) % ALL_COLORS.length;
  return ALL_COLORS[colorIndex];
};

export const hexToRgba = (hex: string, alpha: number): string => {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
};
