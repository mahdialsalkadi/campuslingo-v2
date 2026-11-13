-- Apply after migrations; worker credentials never enter the HTTP process.
GRANT EXECUTE ON FUNCTION can_read_ingestion(UUID),ingestion_asset_allowed(UUID,UUID)
    TO campuslingo_app,campuslingo_worker;
GRANT SELECT ON ingestion_runs,ingestion_pages,ingestion_crops,ingestion_chunks,ingestion_chunk_crops
    TO campuslingo_app;
GRANT SELECT ON courses,assets TO campuslingo_worker;
GRANT INSERT ON assets TO campuslingo_worker;
GRANT SELECT,INSERT,UPDATE ON ingestion_runs TO campuslingo_worker;
GRANT SELECT,INSERT ON ingestion_pages,ingestion_crops,ingestion_chunks,ingestion_chunk_crops TO campuslingo_worker;
