asbuilt validate      # likec4 validate over model.c4 and the curated views
asbuilt export json   # model.json, the same on every machine
asbuilt render        # one SVG per view, via likec4 gen dot and Graphviz
asbuilt docs          # a static HTML tree with a viewer over those SVGs
