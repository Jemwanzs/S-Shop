import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ChevronRight, FileSpreadsheet } from "lucide-react";
import { api } from "@/lib/api";
import { Loading, PageHeader } from "@/components/Page";

export interface ReportMeta {
  key: string;
  title: string;
  group: string;
  description: string;
}

export default function Reports() {
  const { data, isLoading } = useQuery({ queryKey: ["reports"], queryFn: () => api<{ reports: ReportMeta[]; can_export: boolean }>("/reports") });
  if (isLoading || !data) return <Loading />;
  const groups = [...new Set(data.reports.map((r) => r.group))];
  return (
    <>
      <PageHeader eyebrow="Finance" title="Reports" description={`${data.reports.length} standard reports${data.can_export ? " · export to PDF or Excel" : ""}`} />
      <div className="grid gap-5 lg:grid-cols-2 2xl:grid-cols-3">
        {groups.map((g) => (
          <section key={g} className="surface overflow-hidden">
            <h2 className="label-caps border-b bg-muted/40 px-4 py-3">{g}</h2>
            <ul className="divide-y">
              {data.reports.filter((r) => r.group === g).map((r) => (
                <li key={r.key}>
                  <Link to={`/reports/${r.key}`} className="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-accent/40">
                    <FileSpreadsheet className="h-5 w-5 shrink-0 text-primary" />
                    <span className="min-w-0 flex-1">
                      <span className="block font-medium">{r.title}</span>
                      <span className="block truncate text-sm text-muted-foreground">{r.description}</span>
                    </span>
                    <ChevronRight className="h-4 w-4 text-muted-foreground" />
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </>
  );
}
