import React from 'react';

const TaxView: React.FC = () => {
  return (
    <div className="space-y-6">
      <div className="bg-slate-900 border border-slate-800 p-6 rounded-xl">
        <h3 className="text-md font-semibold text-slate-200 mb-4">Extraction Terminal</h3>
        <p className="text-sm text-slate-400 mb-6">Export structured ledger tables for standard CPA parsing.</p>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <button className="bg-slate-950 border border-slate-800 p-4 rounded-lg text-sm text-blue-400 hover:bg-slate-800 transition-colors">
            Export W-2 Module
          </button>
          <button className="bg-slate-950 border border-slate-800 p-4 rounded-lg text-sm text-blue-400 hover:bg-slate-800 transition-colors">
            Export 1099-B / DIV
          </button>
          <button className="bg-emerald-600 p-4 rounded-lg text-sm font-bold hover:bg-emerald-500 transition-colors text-slate-950">
            Master Schedule E Export
          </button>
        </div>
      </div>
    </div>
  );
};

export default TaxView;
