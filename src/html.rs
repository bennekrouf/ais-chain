use crate::graph::Graph;
use crate::parser::Workflow;

pub fn generate(graph: &Graph, workflows: &[Workflow], filter: &Option<String>) -> String {
    let edges = graph.all_edges();

    // Collect relevant nodes and edges based on filter
    let mut nodes_set = std::collections::HashSet::new();
    let mut filtered_edges: Vec<(&str, &str, &str)> = Vec::new();

    for (from, to, link) in &edges {
        if let Some(ref f) = filter {
            let fl = f.to_lowercase();
            if !from.to_lowercase().contains(&fl) && !to.to_lowercase().contains(&fl) {
                continue;
            }
        }
        nodes_set.insert(*from);
        nodes_set.insert(*to);
        filtered_edges.push((from, to, link));
    }

    // If no filter, include standalone workflows too
    if filter.is_none() {
        for wf in workflows {
            nodes_set.insert(&wf.name);
        }
    }

    // Build JSON arrays
    let nodes_json: Vec<String> = nodes_set.iter().map(|name| {
        let wf = workflows.iter().find(|w| w.name == **name);
        let trigger = wf.map(|w| {
            w.triggers.iter()
                .map(|t| format!("{}:{}", t.kind, t.target))
                .collect::<Vec<_>>()
                .join(", ")
        }).unwrap_or_default();

        let group = if trigger.contains("http") { "http" }
            else if trigger.contains("queue") { "queue" }
            else if trigger.contains("blob") { "blob" }
            else if trigger.contains("recurrence") { "recurrence" }
            else if name.starts_with("AIS-") { "generic" }
            else { "other" };

        format!(r#"    {{ "id": "{name}", "trigger": "{trigger}", "group": "{group}" }}"#)
    }).collect();

    let edges_json: Vec<String> = filtered_edges.iter().map(|(from, to, link)| {
        let kind = if link.starts_with("queue:") { "queue" }
            else if *link == "EventGrid" { "eventgrid" }
            else if *link == "invoke" { "invoke" }
            else if link.starts_with("function:") { "function" }
            else { "other" };
        format!(r#"    {{ "source": "{from}", "target": "{to}", "label": "{link}", "kind": "{kind}" }}"#)
    }).collect();

    format!(r##"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>ais-chain — Workflow Graph</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ background: #0d1117; overflow: hidden; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; }}
  svg {{ width: 100vw; height: 100vh; }}
  .link {{ fill: none; stroke-opacity: 0.5; }}
  .link.queue {{ stroke: #f0c040; }}
  .link.eventgrid {{ stroke: #c060e0; }}
  .link.invoke {{ stroke: #4080d0; stroke-dasharray: 6 3; }}
  .link.function {{ stroke: #40c080; stroke-dasharray: 4 4; }}
  .link.other {{ stroke: #888; }}
  .link-label {{ font-size: 9px; fill: #8b949e; pointer-events: none; }}
  .node circle {{ stroke-width: 2; cursor: pointer; }}
  .node text {{ fill: #c9d1d9; font-size: 11px; pointer-events: none; }}
  .node.http circle {{ fill: #1a3a2a; stroke: #3fb950; }}
  .node.queue circle {{ fill: #1a2a3a; stroke: #58a6ff; }}
  .node.blob circle {{ fill: #2a2a1a; stroke: #d29922; }}
  .node.recurrence circle {{ fill: #2a1a2a; stroke: #bc8cff; }}
  .node.generic circle {{ fill: #1a1a2a; stroke: #8b949e; }}
  .node.other circle {{ fill: #1a2020; stroke: #6e7681; }}
  .node.highlighted circle {{ stroke-width: 4; filter: drop-shadow(0 0 8px rgba(255,255,255,0.3)); }}
  .tooltip {{ position: fixed; background: #161b22; border: 1px solid #30363d; color: #c9d1d9; padding: 8px 12px; border-radius: 6px; font-size: 12px; pointer-events: none; z-index: 10; }}
  .legend {{ position: fixed; bottom: 16px; left: 16px; background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; color: #8b949e; font-size: 11px; }}
  .legend div {{ margin: 4px 0; display: flex; align-items: center; gap: 8px; }}
  .legend .swatch {{ width: 12px; height: 12px; border-radius: 50%; display: inline-block; }}
  .legend .line-swatch {{ width: 24px; height: 2px; display: inline-block; }}
  h1 {{ position: fixed; top: 12px; left: 16px; color: #58a6ff; font-size: 16px; font-weight: 600; }}
</style>
</head>
<body>
<h1>ais-chain</h1>
<div class="legend">
  <div><span class="swatch" style="background:#3fb950"></span> HTTP trigger</div>
  <div><span class="swatch" style="background:#58a6ff"></span> Queue trigger</div>
  <div><span class="swatch" style="background:#d29922"></span> Blob trigger</div>
  <div><span class="swatch" style="background:#bc8cff"></span> Recurrence</div>
  <div><span class="swatch" style="background:#8b949e"></span> Generic / Shared</div>
  <div style="margin-top:8px"><span class="line-swatch" style="background:#f0c040"></span> Queue link</div>
  <div><span class="line-swatch" style="background:#c060e0"></span> EventGrid</div>
  <div><span class="line-swatch" style="background:#4080d0;opacity:0.5"></span> Invoke (child)</div>
</div>
<svg></svg>
<script src="https://d3js.org/d3.v7.min.js"></script>
<script>
const nodes = [
{nodes}
];
const links = [
{edges}
];

const width = window.innerWidth;
const height = window.innerHeight;

const svg = d3.select("svg");
const g = svg.append("g");

// Zoom
svg.call(d3.zoom()
  .scaleExtent([0.1, 4])
  .on("zoom", (e) => g.attr("transform", e.transform)));

// Arrow markers
const defs = svg.append("defs");
["queue","eventgrid","invoke","function","other"].forEach(kind => {{
  const colors = {{ queue:"#f0c040", eventgrid:"#c060e0", invoke:"#4080d0", function:"#40c080", other:"#888" }};
  defs.append("marker")
    .attr("id", `arrow-${{kind}}`)
    .attr("viewBox", "0 -5 10 10")
    .attr("refX", 22)
    .attr("refY", 0)
    .attr("markerWidth", 6)
    .attr("markerHeight", 6)
    .attr("orient", "auto")
    .append("path")
    .attr("d", "M0,-5L10,0L0,5")
    .attr("fill", colors[kind]);
}});

const simulation = d3.forceSimulation(nodes)
  .force("link", d3.forceLink(links).id(d => d.id).distance(140))
  .force("charge", d3.forceManyBody().strength(-400))
  .force("center", d3.forceCenter(width / 2, height / 2))
  .force("collision", d3.forceCollide().radius(30));

const link = g.append("g")
  .selectAll("line")
  .data(links)
  .join("line")
  .attr("class", d => `link ${{d.kind}}`)
  .attr("stroke-width", d => d.kind === "invoke" ? 1 : 2)
  .attr("marker-end", d => `url(#arrow-${{d.kind}})`);

const linkLabel = g.append("g")
  .selectAll("text")
  .data(links.filter(d => d.kind !== "invoke"))
  .join("text")
  .attr("class", "link-label")
  .text(d => d.label.replace("queue:", ""));

const node = g.append("g")
  .selectAll("g")
  .data(nodes)
  .join("g")
  .attr("class", d => `node ${{d.group}}`)
  .call(d3.drag()
    .on("start", (e, d) => {{ if (!e.active) simulation.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; }})
    .on("drag", (e, d) => {{ d.fx = e.x; d.fy = e.y; }})
    .on("end", (e, d) => {{ if (!e.active) simulation.alphaTarget(0); d.fx = null; d.fy = null; }}));

node.append("circle").attr("r", d => d.group === "generic" ? 6 : 10);

node.append("text")
  .attr("dx", 14)
  .attr("dy", 4)
  .text(d => d.id);

// Tooltip
const tooltip = d3.select("body").append("div").attr("class", "tooltip").style("display", "none");

node.on("mouseover", (e, d) => {{
  tooltip.style("display", "block")
    .html(`<strong>${{d.id}}</strong><br>${{d.trigger || "no trigger"}}`);
}})
.on("mousemove", (e) => {{
  tooltip.style("left", (e.clientX + 12) + "px").style("top", (e.clientY - 20) + "px");
}})
.on("mouseout", () => tooltip.style("display", "none"));

// Highlight connected nodes on click
node.on("click", (e, d) => {{
  const connected = new Set();
  connected.add(d.id);
  links.forEach(l => {{
    if (l.source.id === d.id) connected.add(l.target.id);
    if (l.target.id === d.id) connected.add(l.source.id);
  }});
  node.classed("highlighted", n => connected.has(n.id));
  link.attr("stroke-opacity", l => connected.has(l.source.id) && connected.has(l.target.id) ? 0.9 : 0.1);
  linkLabel.attr("fill-opacity", l => connected.has(l.source.id) && connected.has(l.target.id) ? 1 : 0.1);
  node.select("text").attr("fill-opacity", n => connected.has(n.id) ? 1 : 0.2);
  node.select("circle").attr("fill-opacity", n => connected.has(n.id) ? 1 : 0.15);
}});

// Double-click to reset
svg.on("dblclick", () => {{
  node.classed("highlighted", false);
  link.attr("stroke-opacity", 0.5);
  linkLabel.attr("fill-opacity", 1);
  node.select("text").attr("fill-opacity", 1);
  node.select("circle").attr("fill-opacity", 1);
}});

simulation.on("tick", () => {{
  link
    .attr("x1", d => d.source.x)
    .attr("y1", d => d.source.y)
    .attr("x2", d => d.target.x)
    .attr("y2", d => d.target.y);
  linkLabel
    .attr("x", d => (d.source.x + d.target.x) / 2)
    .attr("y", d => (d.source.y + d.target.y) / 2);
  node.attr("transform", d => `translate(${{d.x}},${{d.y}})`);
}});
</script>
</body>
</html>"##,
        nodes = nodes_json.join(",\n"),
        edges = edges_json.join(",\n"),
    )
}
