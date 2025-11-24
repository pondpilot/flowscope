import { useState } from 'react';
import { ChevronRight, ChevronDown, Copy, Check } from 'lucide-react';

interface JsonTreeViewProps {
  data: unknown;
  name?: string;
  depth?: number;
}

type JsonValue = string | number | boolean | null | JsonObject | JsonArray;
type JsonObject = { [key: string]: JsonValue };
type JsonArray = JsonValue[];

export function JsonTreeView({ data, name, depth = 0 }: JsonTreeViewProps) {
  const [isExpanded, setIsExpanded] = useState(depth < 2);
  const [copied, setCopied] = useState(false);

  const handleCopy = async (value: unknown) => {
    try {
      await navigator.clipboard.writeText(JSON.stringify(value, null, 2));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  const getValueType = (value: unknown): string => {
    if (value === null) return 'null';
    if (Array.isArray(value)) return 'array';
    return typeof value;
  };

  const getValueColor = (type: string): string => {
    switch (type) {
      case 'string':
        return 'text-green-600';
      case 'number':
        return 'text-blue-600';
      case 'boolean':
        return 'text-purple-600';
      case 'null':
        return 'text-gray-500';
      default:
        return 'text-gray-900';
    }
  };

  const renderValue = (value: unknown, key?: string) => {
    const type = getValueType(value);
    const colorClass = getValueColor(type);

    if (type === 'object' && value !== null) {
      const obj = value as JsonObject;
      const keys = Object.keys(obj);
      const isEmpty = keys.length === 0;

      return (
        <div className="ml-4">
          <div className="flex items-center gap-2 group">
            <button
              onClick={() => setIsExpanded(!isExpanded)}
              className="flex items-center gap-1 hover:bg-gray-100 rounded px-1"
            >
              {isExpanded ? (
                <ChevronDown className="w-4 h-4 text-gray-500" />
              ) : (
                <ChevronRight className="w-4 h-4 text-gray-500" />
              )}
              {key && <span className="font-semibold text-gray-700">{key}:</span>}
              <span className="text-gray-500">
                {isEmpty ? '{}' : `{${keys.length}}`}
              </span>
            </button>
            <button
              onClick={() => handleCopy(value)}
              className="opacity-0 group-hover:opacity-100 p-1 hover:bg-gray-200 rounded transition-opacity"
              title="Copy object"
            >
              {copied ? (
                <Check className="w-3 h-3 text-green-600" />
              ) : (
                <Copy className="w-3 h-3 text-gray-600" />
              )}
            </button>
          </div>
          {isExpanded && !isEmpty && (
            <div className="ml-4 border-l border-gray-200 pl-2">
              {keys.map((k) => (
                <JsonTreeView key={k} data={obj[k]} name={k} depth={depth + 1} />
              ))}
            </div>
          )}
        </div>
      );
    }

    if (type === 'array') {
      const arr = value as JsonArray;
      const isEmpty = arr.length === 0;

      return (
        <div className="ml-4">
          <div className="flex items-center gap-2 group">
            <button
              onClick={() => setIsExpanded(!isExpanded)}
              className="flex items-center gap-1 hover:bg-gray-100 rounded px-1"
            >
              {isExpanded ? (
                <ChevronDown className="w-4 h-4 text-gray-500" />
              ) : (
                <ChevronRight className="w-4 h-4 text-gray-500" />
              )}
              {key && <span className="font-semibold text-gray-700">{key}:</span>}
              <span className="text-gray-500">
                {isEmpty ? '[]' : `[${arr.length}]`}
              </span>
            </button>
            <button
              onClick={() => handleCopy(value)}
              className="opacity-0 group-hover:opacity-100 p-1 hover:bg-gray-200 rounded transition-opacity"
              title="Copy array"
            >
              {copied ? (
                <Check className="w-3 h-3 text-green-600" />
              ) : (
                <Copy className="w-3 h-3 text-gray-600" />
              )}
            </button>
          </div>
          {isExpanded && !isEmpty && (
            <div className="ml-4 border-l border-gray-200 pl-2">
              {arr.map((item, idx) => (
                <JsonTreeView
                  key={idx}
                  data={item}
                  name={`[${idx}]`}
                  depth={depth + 1}
                />
              ))}
            </div>
          )}
        </div>
      );
    }

    return (
      <div className="ml-4 flex items-center gap-2">
        {key && <span className="font-semibold text-gray-700">{key}:</span>}
        <span className={colorClass}>
          {type === 'string' ? `"${value}"` : String(value)}
        </span>
      </div>
    );
  };

  return <div className="py-0.5">{renderValue(data, name)}</div>;
}
