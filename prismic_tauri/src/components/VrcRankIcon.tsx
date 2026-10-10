import React from 'react';

interface VrcRankIconProps {
  rank: number; // 1 = Excellent, 2 = Good, 3 = Medium, 4 = Poor, 5 = Very Poor
  className?: string;
}

export const VrcRankIcon: React.FC<VrcRankIconProps> = ({ rank, className = 'w-5 h-5' }) => {
  switch (rank) {
    case 1:
      // Excellent: Solid lime-green circle with dark star
      return (
        <svg viewBox="0 0 100 100" className={className} aria-label="Excellent Performance Rank">
          <circle cx="50" cy="50" r="46" fill="#48db2c" />
          <polygon
            points="50,22 58.5,39.5 78,42 63.8,56 67.5,75 50,65.5 32.5,75 36.2,56 22,42 41.5,39.5"
            fill="#222f3e"
          />
        </svg>
      );

    case 2:
      // Good: Green ring with small green center dot
      return (
        <svg viewBox="0 0 100 100" className={className} aria-label="Good Performance Rank">
          <circle cx="50" cy="50" r="46" fill="#252b35" stroke="#48db2c" strokeWidth="9" />
          <circle cx="50" cy="50" r="9" fill="#48db2c" />
        </svg>
      );

    case 3:
      // Medium: Amber ring with medium amber center circle
      return (
        <svg viewBox="0 0 100 100" className={className} aria-label="Medium Performance Rank">
          <circle cx="50" cy="50" r="46" fill="#252b35" stroke="#ffa502" strokeWidth="9" />
          <circle cx="50" cy="50" r="22" fill="#ffa502" />
        </svg>
      );

    case 4:
      // Poor: Red-orange ring with large red-orange center circle
      return (
        <svg viewBox="0 0 100 100" className={className} aria-label="Poor Performance Rank">
          <circle cx="50" cy="50" r="46" fill="#252b35" stroke="#e05638" strokeWidth="9" />
          <circle cx="50" cy="50" r="32" fill="#e05638" />
        </svg>
      );

    case 5:
      // Very Poor: Red ring with red exclamation mark !
      return (
        <svg viewBox="0 0 100 100" className={className} aria-label="Very Poor Performance Rank">
          <circle cx="50" cy="50" r="46" fill="#252b35" stroke="#eb4d4b" strokeWidth="9" />
          <path d="M45 28 H55 L53.5 58 H46.5 Z" fill="#eb4d4b" />
          <circle cx="50" cy="69" r="5.5" fill="#eb4d4b" />
        </svg>
      );

    default:
      // 0 / Unknown: Gray ring with subtle dot
      return (
        <svg viewBox="0 0 100 100" className={className} aria-label="Unrated Performance Rank">
          <circle cx="50" cy="50" r="46" fill="#252b35" stroke="#636e72" strokeWidth="9" />
          <circle cx="50" cy="50" r="6" fill="#636e72" />
        </svg>
      );
  }
};
