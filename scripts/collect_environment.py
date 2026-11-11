from campuslingo_ai.environment import environment_report
from campuslingo_ai.reporting import write_report

if __name__ == "__main__":
    report = environment_report()
    path = write_report("private/environment.json", report)
    print(f"Environment evidence: {path}")
    print(f"Available RAM: {report['memory']['available'] / 1024**3:.2f} GiB")
    print(f"Free disk: {report['disk']['free'] / 1024**3:.2f} GiB")
    print(f"Training: {report['training']['status']}")
