import { useRef, useState, useEffect } from 'react';
import { useLineage } from '@pondpilot/flowscope-react';
import {
  GraphView,
  IssuesPanel,
  SummaryBar,
  SchemaView,
  ColumnPanel,
  ViewModeSelector,
} from '@pondpilot/flowscope-react';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/components/ui/tabs';
import { Input } from '@/components/ui/input';
import { ExportMenu } from '@/components/ExportMenu';
import { Search } from 'lucide-react';
import type { SchemaTable } from '@pondpilot/flowscope-core';

const MOCK_SCHEMA: SchemaTable[] = [
  {
    name: 'users',
    columns: [
      { name: 'id', dataType: 'integer' },
      { name: 'name', dataType: 'varchar' },
      { name: 'email', dataType: 'varchar' },
      { name: 'created_at', dataType: 'timestamp' },
    ],
  },
  {
    name: 'orders',
    columns: [
      { name: 'id', dataType: 'integer' },
      { name: 'user_id', dataType: 'integer' },
      { name: 'total', dataType: 'decimal' },
      { name: 'created_at', dataType: 'timestamp' },
    ],
  },
  {
    name: 'products',
    columns: [
      { name: 'id', dataType: 'integer' },
      { name: 'name', dataType: 'varchar' },
      { name: 'price', dataType: 'decimal' },
    ],
  },
];

/**
 * Main analysis view component showing lineage graph, schema, and details.
 * Implements debounced search for optimal performance with large graphs.
 */
export function AnalysisView() {
  const { state, actions } = useLineage();
  const { searchTerm, result } = state;
  const graphContainerRef = useRef<HTMLDivElement>(null);

  // Local state for immediate input feedback
  const [localSearchTerm, setLocalSearchTerm] = useState(searchTerm);

  // Debounce the search term update to avoid performance issues on large graphs
  useEffect(() => {
    const debounceTimer = setTimeout(() => {
      actions.setSearchTerm(localSearchTerm);
    }, 300); // 300ms debounce delay

    return () => {
      clearTimeout(debounceTimer);
    };
  }, [localSearchTerm, actions]);

  // Sync local state when external searchTerm changes
  useEffect(() => {
    setLocalSearchTerm(searchTerm);
  }, [searchTerm]);

  const handleSearchChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setLocalSearchTerm(event.target.value);
  };

  if (!result) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground bg-muted/5">
        <div className="p-6 text-center">
          <h3 className="font-semibold mb-2">No Analysis Results</h3>
          <p className="text-sm max-w-xs mx-auto">
            Run analysis on your SQL script to see lineage and schema details here.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Analysis Header */}
      <div className="border-b px-4 py-2 flex items-center justify-between bg-muted/10">
        <div className="flex-1 font-medium text-sm text-muted-foreground">
          Lineage Graph
        </div>
        <div className="flex items-center gap-3">
          <ViewModeSelector />
          <div className="relative w-[200px]">
            <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3 w-3 text-muted-foreground" />
            <Input
              type="text"
              placeholder="Search nodes..."
              className="pl-8 h-7 text-xs"
              value={localSearchTerm}
              onChange={handleSearchChange}
            />
          </div>
          <ExportMenu graphContainerRef={graphContainerRef} />
        </div>
      </div>

      <SummaryBar />

      <Tabs defaultValue="lineage" className="flex-1 flex flex-col min-h-0">
        <div className="px-4 border-b">
          <TabsList className="h-9 w-full justify-start bg-transparent p-0">
            <TabsTrigger 
              value="lineage" 
              className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-4"
            >
              Lineage
            </TabsTrigger>
            <TabsTrigger 
              value="details" 
              className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-4"
            >
              Details
            </TabsTrigger>
            <TabsTrigger 
              value="schema" 
              className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-4"
            >
              Schema
            </TabsTrigger>
             <TabsTrigger 
              value="issues" 
              className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-4"
            >
              Issues
            </TabsTrigger>
          </TabsList>
        </div>

        <div className="flex-1 overflow-hidden relative">
          <TabsContent value="lineage" className="h-full mt-0 p-0 absolute inset-0">
             <GraphView graphContainerRef={graphContainerRef} className="h-full w-full" />
          </TabsContent>
          
          <TabsContent value="details" className="h-full mt-0 overflow-auto p-0 absolute inset-0">
             <ColumnPanel />
          </TabsContent>

          <TabsContent value="schema" className="h-full mt-0 p-0 absolute inset-0">
            <SchemaView schema={MOCK_SCHEMA} />
          </TabsContent>
          
          <TabsContent value="issues" className="h-full mt-0 overflow-auto p-0 absolute inset-0">
            <IssuesPanel />
          </TabsContent>
        </div>
      </Tabs>
    </div>
  );
}