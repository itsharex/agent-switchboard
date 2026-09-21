import { Button } from "./Button";

interface PaginationProps {
  /** Rows across all pages; a single page renders nothing. */
  total: number;
  /** Current 1-based page. */
  page: number;
  pageSize: number;
  onPageChange: (page: number) => void;
  /** Accessible name; one screen may hold more than one paginated table. */
  label?: string;
}

/** The shared page-turn control for data tables. Hidden while one page holds
 * every row: nothing to turn is not a disabled control. */
export function Pagination({ total, page, pageSize, onPageChange, label = "分页" }: PaginationProps) {
  if (total <= pageSize) return null;
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  return (
    <nav className="asb-pagination" aria-label={label}>
      <Button variant="secondary" disabled={page <= 1} onClick={() => onPageChange(page - 1)}>
        上一页
      </Button>
      <span className="asb-pagination-status" role="status">
        第 {page} / {pageCount} 页
      </span>
      <Button variant="secondary" disabled={page >= pageCount} onClick={() => onPageChange(page + 1)}>
        下一页
      </Button>
    </nav>
  );
}
